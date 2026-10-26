//! Collects image relationships, physical media, and footnotes for packaging.
//!
//! OPC relationships belong to individual XML parts, but media files belong
//! to the whole package. This module models those scopes separately so shared
//! image bytes are written once without losing any part-local relationship.

use std::collections::{HashMap, HashSet};

use super::document_tree::{visit_document, visit_footer, visit_footnotes, visit_header, DocumentTreeVisitor};
use crate::docx::{Document, Footer, Footnote, FootnoteReference, Header, ImageIdAndBuf, ImageIdAndPath, Pic};

/// Stores physical media once for the entire OPC package.
///
/// Relationship IDs are scoped to individual XML parts, while files under
/// `word/media` are package-global. Keeping those concepts separate allows a
/// body, header, and footer to reference the same bytes without writing three
/// copies or dropping a relationship from one of the parts.
#[derive(Default)]
pub(crate) struct MediaRegistry {
    media: Vec<ImageIdAndBuf>,
    media_by_id: HashMap<String, usize>,
    by_fingerprint: HashMap<(usize, u32), Vec<usize>>,
    next_suffix_by_id: HashMap<String, usize>,
}

impl MediaRegistry {
    /// Registers physical bytes and returns their stable registry index.
    ///
    /// Part collectors use the numeric index as their deduplication key. This
    /// avoids cloning and hashing a `media/<id>.png` target for every repeated
    /// picture while keeping the package-visible media ID encapsulated here.
    /// Reused picture IDs are checked before calculating a content fingerprint;
    /// byte equality still protects callers that reuse an ID for new content.
    fn register(&mut self, preferred_id: &str, bytes: std::sync::Arc<[u8]>) -> usize {
        if let Some(index) = self.media_by_id.get(preferred_id).copied() {
            if std::sync::Arc::ptr_eq(&self.media[index].1, &bytes) || self.media[index].1 == bytes {
                return index;
            }
        }

        let fingerprint = (bytes.len(), fnv1a(&bytes));
        if let Some(index) = self
            .by_fingerprint
            .get(&fingerprint)
            .and_then(|candidates| candidates.iter().copied().find(|index| self.media[*index].1 == bytes))
        {
            return index;
        }

        let media_id = self.unique_media_id(preferred_id);
        let index = self.media.len();
        self.media.push((media_id.clone(), bytes));
        self.media_by_id.insert(media_id, index);
        self.by_fingerprint.entry(fingerprint).or_default().push(index);
        index
    }

    /// Returns the package-visible ID assigned to registered media.
    fn media_id(&self, index: usize) -> &str {
        &self.media[index].0
    }

    /// Consumes the registry and returns files ready for ZIP packaging.
    pub(crate) fn into_media(self) -> Vec<ImageIdAndBuf> {
        self.media
    }

    /// Returns a free media ID without restarting collision scans at `_2`.
    ///
    /// Callers can reuse a preferred ID for different bytes. Remembering the
    /// next suffix keeps a run of such collisions linear while still checking
    /// explicitly registered suffixed IDs.
    fn unique_media_id(&mut self, preferred_id: &str) -> String {
        if !self.media_by_id.contains_key(preferred_id) {
            return preferred_id.to_owned();
        }

        let (media_by_id, next_suffix_by_id) = (&self.media_by_id, &mut self.next_suffix_by_id);
        let suffix = next_suffix_by_id.entry(preferred_id.to_owned()).or_insert(2);

        loop {
            let candidate = format!("{preferred_id}_{suffix}");
            *suffix = suffix
                .checked_add(1)
                .expect("the media ID suffix space should not be exhausted");
            if !media_by_id.contains_key(&candidate) {
                return candidate;
            }
        }
    }
}

/// Data collected from one relationship scope during a tree walk.
pub(crate) struct CollectedPart {
    pub(crate) relationships: Vec<ImageIdAndPath>,
    /// External hyperlinks (relationship id, target) met in this part.
    pub(crate) hyperlinks: Vec<(String, String)>,
    pub(crate) footnotes: Vec<Footnote>,
}

/// Collects media relationships and footnotes while visiting one XML part.
///
/// A fresh collector is created for each relationship scope, but every
/// collector shares one [`MediaRegistry`]. This is the key distinction that
/// preserves per-part relationships while deduplicating physical media.
struct PackagePartCollector<'a> {
    registry: &'a mut MediaRegistry,
    relationship_prefix: Option<&'a str>,
    relationships: Vec<ImageIdAndPath>,
    relationship_ids: HashSet<String>,
    /// Continues collision scans without revisiting previously used suffixes.
    next_relationship_suffix_by_id: HashMap<String, usize>,
    /// Maps media to the owning relationship entry, avoiding another owned ID.
    relationships_by_media: HashMap<usize, usize>,
    footnotes: Vec<Footnote>,
    hyperlinks: Vec<(String, String)>,
    seen_links: HashSet<String>,
}

impl<'a> PackagePartCollector<'a> {
    fn new(registry: &'a mut MediaRegistry, relationship_prefix: Option<&'a str>) -> Self {
        Self {
            registry,
            relationship_prefix,
            relationships: Vec::new(),
            relationship_ids: HashSet::new(),
            next_relationship_suffix_by_id: HashMap::new(),
            relationships_by_media: HashMap::new(),
            footnotes: Vec::new(),
            hyperlinks: Vec::new(),
            seen_links: HashSet::new(),
        }
    }

    fn finish(self) -> CollectedPart {
        CollectedPart {
            relationships: self.relationships,
            hyperlinks: self.hyperlinks,
            footnotes: self.footnotes,
        }
    }

    fn relationship_id(&self, picture_id: &str) -> String {
        match self.relationship_prefix {
            Some(prefix) => format!("{prefix}{picture_id}"),
            None => picture_id.to_owned(),
        }
    }

    /// Returns a unique part-local relationship ID in amortized linear time.
    ///
    /// Repeated preferred IDs use the next unchecked suffix instead of
    /// restarting at `_2`, while explicit suffixed IDs are still skipped.
    fn unique_relationship_id(&mut self, preferred_id: &str) -> String {
        if !self.relationship_ids.contains(preferred_id) {
            return preferred_id.to_owned();
        }

        let (relationship_ids, next_suffix_by_id) = (&self.relationship_ids, &mut self.next_relationship_suffix_by_id);
        let suffix = next_suffix_by_id.entry(preferred_id.to_owned()).or_insert(2);

        loop {
            let candidate = format!("{preferred_id}_{suffix}");
            *suffix = suffix
                .checked_add(1)
                .expect("the relationship ID suffix space should not be exhausted");
            if !relationship_ids.contains(&candidate) {
                return candidate;
            }
        }
    }
}

impl DocumentTreeVisitor for PackagePartCollector<'_> {
    fn visit_picture(&mut self, picture: &mut Pic) {
        let preferred_relationship_id = self.relationship_id(&picture.id);
        let media_index = self
            .registry
            .register(&preferred_relationship_id, std::mem::take(&mut picture.image));

        if let Some(relationship_index) = self.relationships_by_media.get(&media_index) {
            picture.id.clone_from(&self.relationships[*relationship_index].0);
            return;
        }

        let relationship_id = self.unique_relationship_id(&preferred_relationship_id);
        let target = format!("media/{}.png", self.registry.media_id(media_index));
        let relationship_index = self.relationships.len();
        self.relationship_ids.insert(relationship_id.clone());
        self.relationships_by_media.insert(media_index, relationship_index);
        picture.id.clone_from(&relationship_id);
        self.relationships.push((relationship_id, target));
    }

    fn visit_hyperlink(&mut self, link: &crate::docx::Hyperlink) {
        if let crate::docx::HyperlinkData::External { rid, path, .. } = &link.link {
            if self.seen_links.insert(rid.clone()) {
                self.hyperlinks.push((rid.clone(), path.clone()));
            }
        }
    }

    fn visit_footnote_reference(&mut self, reference: &FootnoteReference) {
        self.footnotes.push(reference.into());
    }
}

/// Collects package data from the complete main document tree in one pass.
pub(crate) fn collect_document_part(document: &mut Document, registry: &mut MediaRegistry) -> CollectedPart {
    let mut collector = PackagePartCollector::new(registry, None);
    visit_document(document, &mut collector);
    collector.finish()
}

/// Collects pictures and links used inside footnotes (their own relationship scope).
pub(crate) fn collect_footnote_part(
    footnotes: &mut crate::docx::Footnotes,
    registry: &mut MediaRegistry,
) -> CollectedPart {
    let mut collector = PackagePartCollector::new(registry, Some("footnote"));
    visit_footnotes(footnotes, &mut collector);
    collector.finish()
}

/// Collects package data from one header relationship scope.
pub(crate) fn collect_header_part(header: &mut Header, registry: &mut MediaRegistry) -> CollectedPart {
    let mut collector = PackagePartCollector::new(registry, Some("header"));
    visit_header(header, &mut collector);
    collector.finish()
}

/// Collects package data from one footer relationship scope.
pub(crate) fn collect_footer_part(footer: &mut Footer, registry: &mut MediaRegistry) -> CollectedPart {
    let mut collector = PackagePartCollector::new(registry, Some("footer"));
    visit_footer(footer, &mut collector);
    collector.finish()
}

/// Cheap content fingerprint used only to bucket candidate duplicates; equality is always
/// confirmed byte-for-byte.
fn fnv1a(bytes: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in bytes {
        h ^= u32::from(*b);
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

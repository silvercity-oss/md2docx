//! Mermaid diagrams rendered inside the process, through [`merman`](https://crates.io/crates/merman)
//! (cargo feature `mermaid`).
//!
//! Unlike the external-command engines this starts **no process**: a host that confines its
//! conversion worker to a single process (a Windows job object with an active-process limit of
//! one, a seccomp filter without `execve`) can still render diagrams, nothing is written to the
//! scratch directory, and the render shares the conversion's deadline and cancellation. It is a
//! [`FenceRenderer`], so it is registered like any other:
//!
//! ```
//! # #[cfg(feature = "mermaid")] {
//! use md2docx::{markdown_to_docx, Config};
//!
//! let cfg = Config::default().with_in_process_mermaid();
//! let docx = markdown_to_docx("```mermaid\nflowchart LR\n  A --> B\n```\n", &cfg).unwrap();
//! assert!(docx.starts_with(b"PK"));
//! # }
//! ```
//!
//! # What it does and does not promise
//!
//! * Rendering is parser- and layout-bounded by merman's own resource policy
//!   ([`MermaidProfile`]) and the output by this crate's image limits; the raster is scaled down to
//!   fit [`Limits::max_image_dimension`](crate::Limits) and
//!   [`Limits::max_image_pixels`](crate::Limits) instead of failing.
//! * The conversion's [`CancelToken`](crate::CancelToken) is bridged to merman's cancellation handle
//!   and the remaining time to its deadline, so a long layout is interrupted at merman's next
//!   checkpoint. This is cooperative: it is not a hard guarantee for hostile input, because a
//!   thread cannot be killed from inside its own process. A host that needs a hard deadline
//!   still terminates the worker from outside.
//! * Text is measured with merman's deterministic metrics (no system fonts), so output does not
//!   depend on the machine. Labels are slightly less precise than the `merman-cli` engine's
//!   native-font measurement.
//! * This is the *tested* merman version only ([`MERMAN_VERSION`]); the dependency allows any compatible `0.9.x` (Cargo.lock pins the one CI tests); `merman` types never appear in this crate's API.

use crate::config::Config;
use crate::extend::{FenceError, FenceRenderer, RenderContext};
use crate::CrateError;

use merman::svg::export::{RasterFitBox, RasterOptions, RasterSizeLimit};
use merman::svg::{RenderResourcePolicy, SvgPipeline};
use merman::{
    OperationControl, ParseOptions, PngRequest, RenderError, RenderOutput, RenderRequest, Renderer, SvgEnvironment,
    SvgRequest,
};

/// The merman version this crate's tests ran against (a newer compatible `0.9.x` may be resolved).
pub const MERMAN_VERSION: &str = "0.9.2";

/// How much work merman may spend on one diagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum MermaidProfile {
    /// merman's smallest budget: for diagram text you do not control. Small and medium
    /// diagrams only. Default.
    #[default]
    Constrained,
    /// A larger budget, sized for an interactive preview.
    Interactive,
    /// merman's budget for trusted input.
    Trusted,
}

impl MermaidProfile {
    fn policy(self) -> RenderResourcePolicy {
        match self {
            MermaidProfile::Constrained => RenderResourcePolicy::constrained(),
            MermaidProfile::Interactive => RenderResourcePolicy::interactive(),
            MermaidProfile::Trusted => RenderResourcePolicy::trusted_native(),
        }
    }
}

/// A [`FenceRenderer`] for ```` ```mermaid ```` blocks that renders in the calling thread.
#[derive(Debug, Clone)]
pub struct InProcessMermaid {
    profile: MermaidProfile,
    scale: f32,
}

impl Default for InProcessMermaid {
    fn default() -> Self {
        InProcessMermaid {
            profile: MermaidProfile::default(),
            scale: 3.0,
        }
    }
}

impl InProcessMermaid {
    /// The default: [`MermaidProfile::Constrained`], rendered at 3x (288 dpi at the document's
    /// default `GraphOptions::dpi`).
    pub fn new() -> Self {
        Self::default()
    }

    /// Choose how much work merman may spend on one diagram.
    pub fn with_profile(mut self, profile: MermaidProfile) -> Self {
        self.profile = profile;
        self
    }

    /// Raster scale (default 3.0). The picture is still capped by the conversion's image limits.
    /// Values outside `0.25..=8.0` are clamped.
    pub fn with_scale(mut self, scale: f32) -> Self {
        self.scale = scale.clamp(0.25, 8.0);
        self
    }

    fn render_png(&self, source: &str, ctx: &RenderContext) -> Result<Vec<u8>, FenceError> {
        ctx.check()?;
        let control = OperationControl::new();
        let control = match ctx.remaining() {
            Some(left) => control.with_deadline(left),
            None => control,
        };
        // Cancelling the conversion cancels the render.
        let _bridge = ctx.cancel_token().map(|token| {
            let handle = control.clone();
            token.register(move || handle.cancel())
        });

        let side = ctx.max_image_dimension().min(16_384);
        let raster = RasterOptions::default()
            .with_scale(self.scale)
            .with_background("transparent")
            .with_fit_to(RasterFitBox::contain(side, side))
            .with_size_limit(RasterSizeLimit::new(
                Some(side),
                Some(side),
                Some(ctx.max_image_pixels()),
            ));
        let svg = SvgRequest {
            environment: SvgEnvironment::deterministic().with_resource_policy(self.profile.policy()),
            pipeline: Some(SvgPipeline::resvg_safe()),
            ..SvgRequest::default()
        };
        let renderer = Renderer::new().with_parse_options(ParseOptions::strict());
        let output = renderer
            .render(RenderRequest::png(source, control, PngRequest { svg, options: raster }))
            .map_err(|e| match e {
                RenderError::ResourceLimitExceeded(_) => {
                    Box::new(CrateError::LimitExceeded(format!("diagram: {e}"))) as FenceError
                }
                other => other.to_string().into(),
            })?;
        match output {
            RenderOutput::Png(Some(png)) => Ok(png.bytes),
            _ => Err("the block is not a Mermaid diagram".into()),
        }
    }
}

impl FenceRenderer for InProcessMermaid {
    fn render(&self, source: &str) -> Result<Vec<u8>, FenceError> {
        self.render_png(source, &RenderContext::unbounded())
    }

    fn render_with(&self, source: &str, ctx: &RenderContext) -> Result<Vec<u8>, FenceError> {
        self.render_png(source, ctx)
    }
}

impl Config {
    /// Render ```` ```mermaid ```` blocks in-process with [`InProcessMermaid`] and its defaults.
    /// No process is started, so this is the way to keep diagrams in a conversion worker that may
    /// only run one process. Equivalent to
    /// `with_fence_renderer("mermaid", InProcessMermaid::new())`.
    pub fn with_in_process_mermaid(self) -> Self {
        self.with_fence_renderer("mermaid", InProcessMermaid::new())
    }
}

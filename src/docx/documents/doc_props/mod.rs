mod app;
mod core;
mod custom;

pub use self::app::*;
pub use self::core::*;
pub use self::custom::*;

use crate::docx::documents::BuildXML;

#[derive(Debug, Clone, PartialEq)]
pub struct DocProps {
    pub app: AppProps,
    pub core: CoreProps,
    pub custom: CustomProps,
}

impl DocProps {
    pub(crate) fn new(core_config: CorePropsConfig) -> DocProps {
        let app = AppProps::new();
        let core = CoreProps::new(core_config);
        let custom = CustomProps::new();
        DocProps { app, core, custom }
    }

    pub fn created_at(mut self, date: &str) -> Self {
        self.core = self.core.created_at(date);
        self
    }

    pub fn updated_at(mut self, date: &str) -> Self {
        self.core = self.core.updated_at(date);
        self
    }

    pub(crate) fn build(&self) -> XMLDocProps {
        XMLDocProps {
            app: self.app.build(),
            core: self.core.build(),
            custom: self.custom.build(),
        }
    }
}

#[derive(Debug)]
pub struct XMLDocProps {
    pub app: Vec<u8>,
    pub core: Vec<u8>,
    pub custom: Vec<u8>,
}

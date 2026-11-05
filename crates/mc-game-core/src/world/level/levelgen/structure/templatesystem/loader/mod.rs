//! `templatesystem.loader`: where structure templates are read from.

pub mod directory_template_source;
pub mod resource_manager_template_source;
pub mod template_path_factory;
pub mod template_source;

pub use directory_template_source::DirectoryTemplateSource;
pub use resource_manager_template_source::ResourceManagerTemplateSource;
pub use template_path_factory::TemplatePathFactory;
pub use template_source::TemplateSource;

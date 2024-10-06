use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Config {
    pub output: Output,
    pub style: Style,
    pub toc: Toc,
    pub advanced: Advanced,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Output {
    pub format: OutputFormat,
    pub name: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputFormat {
    Pdf,
    Svg,
    Png,
    #[default]
    Typst,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Style {
    #[serde(default = "default_style_enable")]
    pub enable: Option<bool>,
    #[serde(default = "default_style_simple")]
    pub simple: Option<bool>,
    pub paper: Option<String>,
    pub text_size: Option<String>,
    pub text_font: Option<String>,
    pub paragraph_spacing: Option<String>,
    pub paragraph_leading: Option<String>,
    pub heading_numbering: Option<String>,
    pub heading_below: Option<String>,
    pub heading_above: Option<String>,
    #[serde(default = "default_link_underline")]
    pub link_underline: Option<bool>,
    pub link_color: Option<String>,
}
pub fn default_style_enable() -> Option<bool> {
    Some(true)
}
pub fn default_style_simple() -> Option<bool> {
    Some(false)
}
pub fn default_paper() -> String {
    "us-letter".to_string()
}
pub fn default_text_size() -> String {
    "11pt".to_string()
}
pub fn default_text_font() -> String {
    "Helvetica".to_string()
}
pub fn default_paragraph_spacing() -> String {
    "2em".to_string()
}
pub fn default_paragraph_leading() -> String {
    ".8em".to_string()
}
pub fn default_heading_below() -> String {
    "2em".to_string()
}
pub fn default_heading_above() -> String {
    "2em".to_string()
}
pub fn default_link_underline() -> Option<bool> {
    Some(true)
}
pub fn default_link_color() -> String {
    "blue".to_string()
}


#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Toc {
    #[serde(default = "default_toc_enable")]
    pub enable: Option<bool>,
    pub depth: Option<u8>,
    pub indent: Option<String>,
    pub entry_show_rules: Option<Vec<TocEntryShowRule>>,
}

pub fn default_toc_enable() -> Option<bool> {
    Some(true)
}
pub fn default_toc_depth() -> u8 {
    2
}
pub fn default_toc_indent() -> String {
    "2em".to_string()
}
pub fn default_toc_entry_show_rules() -> Option<Vec<TocEntryShowRule>> {
    Some(vec![TocEntryShowRule {
        level: default_toc_entry_level(),
        text_size: default_toc_entry_text_size(),
        strong: default_toc_entry_bold(),
    }])
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct TocEntryShowRule {
    #[serde(default = "default_toc_entry_level")]
    pub level: Option<u8>,
    pub text_size: Option<String>,
    pub strong: Option<bool>,
}

pub fn default_toc_entry_level() -> Option<u8> {
    Some(1)
}
pub fn default_toc_entry_text_size() -> Option<String> {
    Some("11pt".to_string())
}
pub fn default_toc_entry_bold() -> Option<bool> {
    Some(true)
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Advanced {
    pub typst_markup_header: Option<String>,
    pub typst_markup_footer: Option<String>,
}

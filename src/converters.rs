use std::marker::PhantomData;
use std::collections::HashMap;
use regex::Regex;

use pullup::converter;
use pullup::markdown::{Event as MdEvent, Tag as MdTag};
use pullup::mdbook::{Event as MdbookEvent, Tag as MdbookTag};
use pullup::typst::{Event as TypstEvent, Tag as TypstTag};
use pullup::ParserEvent;

/// Convert markdown footnote to Typst footnotes.

pub fn process_events<'a>(events: impl Iterator<Item = ParserEvent<'a>>) -> Vec<ParserEvent<'a>> {
    let mut footnote_definitions = HashMap::new();
    let mut processed_events = vec![];

    // First pass: Collect footnote definitions
    for event in events {
        if let ParserEvent::Typst(TypstEvent::Text(ref text)) = event {
            let re = Regex::new(r"\[\^([^\s\]]+)\]:\s*(.*)").unwrap();  // Compile regex pattern to match any word (not just digits)

            if let Some(captures) = re.captures(text) {
                let footnote_label = captures[1].to_string();
                let footnote_content = captures[2].to_string();
                footnote_definitions.insert(footnote_label, footnote_content);
                continue;
            }
        }
        processed_events.push(event);
    }

    // Second pass: Replace footnote references
    let mut final_events = Vec::new();
    for event in processed_events {
        if let ParserEvent::Typst(TypstEvent::Text(ref text)) = event {
            let re = Regex::new(r"\[\^([^\s\]]+)\]").unwrap();  // Compile regex pattern to match any word (not just digits)

            let updated_text = re.replace_all(text, |caps: &regex::Captures| {
                let label = &caps[1];
                if let Some(content) = footnote_definitions.get(label) {
                    let footnote_reference = format!("#footnote[{}]", content);  // Format the footnote correctly
                    // println!("Footnote Reference: {}", footnote_reference);  // Print the footnote reference
                    footnote_reference  // Return the formatted reference
                    //format!(r"#footnote[{}]", content)
                } else {
                    // println!("Footnote not found for label: {}", label);  // Print a message if not found
                    caps[0].to_string() // Fallback if footnote not found
                }
            });

            // Ensure ownership of the updated_text
            // println!("Updated Text: {}", updated_text);  // Print the updated text
            final_events.push(ParserEvent::Typst(TypstEvent::Text(updated_text.into_owned().into())));
        } else {
            final_events.push(event);
        }
    }

    final_events
}

/// Convert mdBook parts to chapters with cover pages.
#[derive(Debug)]
pub struct PartToCoverPage<'a, T> {
    in_part: bool,
    iter: T,
    _p: PhantomData<&'a ()>,
}

impl<'a, T> PartToCoverPage<'a, T>
where
    T: Iterator<Item = ParserEvent<'a>>,
{
    #[allow(dead_code)]
    pub fn new(iter: T) -> Self {
        Self {
            in_part: false,
            iter,
            _p: PhantomData,
        }
    }
}

impl<'a, T> Iterator for PartToCoverPage<'a, T>
where
    T: Iterator<Item = ParserEvent<'a>>,
{
    type Item = ParserEvent<'a>;

    fn next(&mut self) -> Option<Self::Item> {

        // for debugging uncomment below:
        // let next_event = self.iter.next();
        // println!("2. {:?}", next_event);
        // match (self.in_part, next_event) {
        match (self.in_part, self.iter.next()) {
            (_, Some(ParserEvent::Mdbook(MdbookEvent::Start(MdbookTag::Part(name, _))))) => {
                if let Some(name) = name {
                    self.in_part = true;
                    Some(ParserEvent::Typst(TypstEvent::Raw(
                        format!(
                            r#"
                        #set page(
                            header: none,
                        )
                        #heading(level: 1, outlined: true, "{}")
                        #pagebreak()
                        #set page(
                            header:  text(size: 8pt, fill: gray)[#align(right)[{}]],
                        )
                        {}"#,
                            name, name, '\n'
                        )
                        .into(),
                    )))
                } else {
                    self.in_part = false;
                    self.next()
                }
            }
            (_, Some(ParserEvent::Mdbook(MdbookEvent::End(MdbookTag::Part(_, _))))) => {
                self.in_part = false;
                Some(ParserEvent::Typst(TypstEvent::FunctionCall(
                    None,
                    "pagebreak".into(),
                    vec!["weak: true".into()],
                )))
            }
            (
                true,
                Some(ParserEvent::Typst(TypstEvent::Start(TypstTag::Heading(num, toc, bookmarks)))),
            ) => Some(ParserEvent::Typst(TypstEvent::Start(TypstTag::Heading(
                num.saturating_add(1),
                toc,
                bookmarks,
            )))),
            (
                true,
                Some(ParserEvent::Typst(TypstEvent::End(TypstTag::Heading(num, toc, bookmarks)))),
            ) => Some(ParserEvent::Typst(TypstEvent::End(TypstTag::Heading(
                num.saturating_add(1),
                toc,
                bookmarks,
            )))),
            // Here, we replace Parbreak with the correct raw event.
            (_, Some(ParserEvent::Typst(TypstEvent::Parbreak))) => {
                Some(ParserEvent::Typst(TypstEvent::Raw("\\\n".into())))
            },
            // fix quotes ending with a newline
            (_, Some(ParserEvent::Typst(TypstEvent::End(TypstTag::Quote(..))))) => {
                Some(ParserEvent::Typst(TypstEvent::Raw("]\n".into()))) // No newline here
            },
            // Detect horizontal rule (hr) in markdown.
            (_, Some(ParserEvent::Markdown(MdEvent::Rule))) => {
                Some(ParserEvent::Typst(TypstEvent::Raw(
                    "#align(center, line(length: 60%))\n".into())))
            },
            (_, x) => x,
        }
    }
}

/// Convert mdBook parts to chapters with cover pages. SIMPLE VERSION
#[derive(Debug)]
pub struct PartToCoverPageSimple<'a, T> {
    in_part: bool,
    iter: T,
    _p: PhantomData<&'a ()>,
}

impl<'a, T> PartToCoverPageSimple<'a, T>
where
    T: Iterator<Item = ParserEvent<'a>>,
{
    #[allow(dead_code)]
    pub fn new(iter: T) -> Self {
        Self {
            in_part: false,
            iter,
            _p: PhantomData,
        }
    }
}

impl<'a, T> Iterator for PartToCoverPageSimple<'a, T>
where
    T: Iterator<Item = ParserEvent<'a>>,
{
    type Item = ParserEvent<'a>;

    fn next(&mut self) -> Option<Self::Item> {

        // for debugging uncomment below:
        // let next_event = self.iter.next();
        // println!("2. {:?}", next_event);
        // match (self.in_part, next_event) {
        match (self.in_part, self.iter.next()) {
            (_, Some(ParserEvent::Mdbook(MdbookEvent::Start(MdbookTag::Part(name, _))))) => {
                if let Some(name) = name {
                    self.in_part = true;
                    Some(ParserEvent::Typst(TypstEvent::Raw(
                        format!(
                            r#"={} {}"#,
                            name, '\n'
                        )
                        .into(),
                    )))
                } else {
                    self.in_part = false;
                    self.next()
                }
            }
            (_, Some(ParserEvent::Mdbook(MdbookEvent::End(MdbookTag::Part(_, _))))) => {
                self.in_part = false;
                Some(ParserEvent::Typst(TypstEvent::FunctionCall(
                    None,
                    "pagebreak".into(),
                    vec!["weak: true".into()],
                )))
            }
            (
                true,
                Some(ParserEvent::Typst(TypstEvent::Start(TypstTag::Heading(num, toc, bookmarks)))),
            ) => Some(ParserEvent::Typst(TypstEvent::Start(TypstTag::Heading(
                num.saturating_add(1),
                toc,
                bookmarks,
            )))),
            (
                true,
                Some(ParserEvent::Typst(TypstEvent::End(TypstTag::Heading(num, toc, bookmarks)))),
            ) => Some(ParserEvent::Typst(TypstEvent::End(TypstTag::Heading(
                num.saturating_add(1),
                toc,
                bookmarks,
            )))),
            // Here, we replace Parbreak with the correct raw event.
            (_, Some(ParserEvent::Typst(TypstEvent::Parbreak))) => {
                Some(ParserEvent::Typst(TypstEvent::Raw("\\\n".into())))
            },
            // fix quotes ending with a newline
            (_, Some(ParserEvent::Typst(TypstEvent::End(TypstTag::Quote(..))))) => {
                Some(ParserEvent::Typst(TypstEvent::Raw("]\n".into()))) // No newline here
            },
            // Detect horizontal rule (hr) in markdown.
            (_, Some(ParserEvent::Markdown(MdEvent::Rule))) => {
                Some(ParserEvent::Typst(TypstEvent::Raw(
                    "#align(center, line(length: 60%))\n".into())))
            },
            (_, x) => x,
        }
    }
}

/// Removes back to back headers.
// FIXME: Should only remove the same level and/or the same content.
#[derive(Debug)]
pub struct FixHeadingStutter<'a, T> {
    prev: Option<ParserEvent<'a>>,
    iter: T,
}

impl<'a, T> FixHeadingStutter<'a, T>
where
    T: Iterator<Item = ParserEvent<'a>>,
{
    #[allow(dead_code)]
    pub fn new(iter: T) -> Self {
        Self { prev: None, iter }
    }
}

impl<'a, T> Iterator for FixHeadingStutter<'a, T>
where
    T: Iterator<Item = ParserEvent<'a>>,
{
    type Item = ParserEvent<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match (&mut self.prev, self.iter.next()) {
            (
                Some(
                    event @ ParserEvent::Mdbook(MdbookEvent::MarkdownContentEvent(MdEvent::End(
                        MdTag::Heading(_, _, _),
                    ))),
                ),
                Some(ParserEvent::Mdbook(MdbookEvent::MarkdownContentEvent(MdEvent::Start(
                    MdTag::Heading(_, _, _),
                )))),
            ) => {
                self.prev = Some(event.clone());
                let _ = self.iter.find(|x| {
                    !matches!(
                        x,
                        ParserEvent::Mdbook(MdbookEvent::MarkdownContentEvent(MdEvent::End(
                            MdTag::Heading(_, _, _)
                        ),)),
                    )
                });
                self.iter.next()
            }
            (
                event @ Some(ParserEvent::Typst(TypstEvent::End(TypstTag::Heading(_, _, _)))),
                Some(ParserEvent::Typst(TypstEvent::Start(TypstTag::Heading(_, _, _)))),
            ) => {
                self.prev = event.clone();
                let _ = self.iter.find(|x| {
                    matches!(
                        x,
                        ParserEvent::Typst(TypstEvent::End(TypstTag::Heading(_, _, _)))
                    )
                });
                self.iter.next()
            }
            (_, x) => {
                self.prev = x.clone();
                x
            }
        }
    }
}

converter!(
    /// Convert parts to cover pages.
    PartToCoverPageX,
    ParserEvent<'a> => ParserEvent<'a>,
    |this: &mut Self| {
        match this.iter.next() {
            Some(ParserEvent::Mdbook(MdbookEvent::Start(MdbookTag::Part(name, _)))) => {
                if let Some(name) = name {
                    Some(ParserEvent::Typst(TypstEvent::Raw(
                        format!(
                        r#"
                        #set page(
                            header: none,
                        )
                        #heading(level: 1, outlined: false, "{}")
                        #pagebreak()
                        #set page(
                            header:  text(size: 8pt, fill: gray)[#align(right)[{}]],
                        )
                        {}"#, name, name, '\n').into()))
                    )
                } else {
                    this.next()
                }
            },
            Some(ParserEvent::Mdbook(MdbookEvent::End(MdbookTag::Part( _, _)))) => {
                Some(ParserEvent::Typst(TypstEvent::FunctionCall(
                    None,
                    "pagebreak".into(),
                    vec!["weak: true".into()],
                )))
            },
            x => x,
    }
});

//! Structured Markdown footnote support.
//!
//! mdBook's bundled parser does not enable pulldown-cmark's footnote
//! extension, so this module builds the mdBook event stream with that option
//! enabled and then turns the resulting structured events into Typst.

use std::collections::HashMap;

use mdbook::{renderer::RenderContext, BookItem};
use pullup::markdown::{
    Event as MarkdownEvent, Options as MarkdownOptions, Parser as MarkdownParser,
};
use pullup::mdbook::{
    ChapterSource, ChapterStatus, ContentType, Event as MdbookEvent, Tag as MdbookTag,
};
use pullup::typst::{Event as TypstEvent, Tag as TypstTag};
use pullup::ParserEvent;

/// Build the mdBook event stream while enabling pulldown-cmark footnotes.
pub fn mdbook_events<'a>(ctx: &'a RenderContext) -> Vec<MdbookEvent<'a>> {
    let mut events = Vec::new();
    events.push(MdbookEvent::Start(MdbookTag::BookConfiguration));
    events.push(MdbookEvent::Root(ctx.root.clone()));

    if let Some(title) = ctx.config.book.title.as_ref() {
        events.push(MdbookEvent::Title(title.clone().into()));
    }
    if !ctx.config.book.authors.is_empty() {
        events.push(MdbookEvent::Start(MdbookTag::AuthorList));
        for author in &ctx.config.book.authors {
            events.push(MdbookEvent::Author(author.clone().into()));
        }
        events.push(MdbookEvent::End(MdbookTag::AuthorList));
    }
    events.push(MdbookEvent::End(MdbookTag::BookConfiguration));
    events.push(MdbookEvent::Start(MdbookTag::BookContent));

    let has_parts = ctx
        .book
        .sections
        .iter()
        .any(|item| matches!(item, BookItem::PartTitle(_)));
    if !has_parts {
        events.push(MdbookEvent::Start(MdbookTag::Part(None, None)));
        push_items(&ctx.book.sections, &mut events);
        events.push(MdbookEvent::End(MdbookTag::Part(None, None)));
    } else {
        push_items_with_parts(&ctx.book.sections, &mut events);
    }

    events.push(MdbookEvent::End(MdbookTag::BookContent));
    events
}

fn push_items<'a>(items: &'a [BookItem], events: &mut Vec<MdbookEvent<'a>>) {
    for item in items {
        match item {
            BookItem::Chapter(chapter) => push_chapter(chapter, events),
            BookItem::Separator => events.push(MdbookEvent::Separator),
            BookItem::PartTitle(_) => {}
        }
    }
}

fn push_items_with_parts<'a>(items: &'a [BookItem], events: &mut Vec<MdbookEvent<'a>>) {
    let mut current_part: Option<&'a str> = None;
    for item in items {
        match item {
            BookItem::PartTitle(title) => {
                if let Some(previous) = current_part {
                    events.push(MdbookEvent::End(MdbookTag::Part(
                        Some(previous.into()),
                        None,
                    )));
                }
                current_part = Some(title.as_str());
                events.push(MdbookEvent::Start(MdbookTag::Part(
                    Some(title.clone().into()),
                    None,
                )));
            }
            BookItem::Chapter(chapter) => push_chapter(chapter, events),
            BookItem::Separator => events.push(MdbookEvent::Separator),
        }
    }
    if let Some(part) = current_part {
        events.push(MdbookEvent::End(MdbookTag::Part(Some(part.into()), None)));
    }
}

fn push_chapter<'a>(chapter: &'a mdbook::book::Chapter, events: &mut Vec<MdbookEvent<'a>>) {
    let status = if chapter.is_draft_chapter() {
        ChapterStatus::Draft
    } else {
        ChapterStatus::Active
    };
    let name = chapter.name.clone();
    let source = chapter
        .source_path
        .as_ref()
        .map(|path| ChapterSource::Path(path.to_owned()));

    events.push(MdbookEvent::Start(MdbookTag::Chapter(
        status,
        name.clone().into(),
        source.clone(),
        None,
    )));
    if !chapter.content.is_empty() {
        let parser = MarkdownParser::new_ext(
            &chapter.content,
            MarkdownOptions::ENABLE_TABLES | MarkdownOptions::ENABLE_FOOTNOTES,
        );
        for event in parser {
            events.push(MdbookEvent::MarkdownContentEvent(event));
        }
        events.push(MdbookEvent::End(MdbookTag::Content(ContentType::Markdown)));
    }
    if !chapter.sub_items.is_empty() {
        push_items(&chapter.sub_items, events);
    }
    events.push(MdbookEvent::End(MdbookTag::Chapter(
        status,
        name.into(),
        source,
        None,
    )));
}

/// Replace structured Markdown footnote references with Typst footnote calls.
/// The body remains an event sequence, so quotes, images, lists, and emphasis
/// are not flattened into a regex-captured string.
pub fn process_events<'a>(events: impl Iterator<Item = ParserEvent<'a>>) -> Vec<ParserEvent<'a>> {
    let events = events.collect::<Vec<_>>();
    let mut definitions: HashMap<String, Vec<ParserEvent<'a>>> = HashMap::new();
    let mut skipped = vec![false; events.len()];

    let mut index = 0;
    while index < events.len() {
        let label = match &events[index] {
            ParserEvent::Markdown(MarkdownEvent::Start(
                pullup::markdown::Tag::FootnoteDefinition(label),
            )) => Some(label.to_string()),
            _ => None,
        };
        if let Some(label) = label {
            let mut end = index + 1;
            while end < events.len()
                && !matches!(
                    events[end],
                    ParserEvent::Markdown(MarkdownEvent::End(
                        pullup::markdown::TagEnd::FootnoteDefinition,
                    ))
                )
            {
                end += 1;
            }
            if end < events.len() {
                definitions.insert(
                    label,
                    trim_single_paragraph(events[index + 1..end].to_vec()),
                );
                for item in &mut skipped[index..=end] {
                    *item = true;
                }
                index = end;
            }
        }
        index += 1;
    }

    let mut output = Vec::with_capacity(events.len());
    for (index, event) in events.into_iter().enumerate() {
        if skipped[index] {
            continue;
        }
        match event {
            ParserEvent::Markdown(MarkdownEvent::FootnoteReference(label)) => {
                if let Some(body) = definitions.get(label.as_ref()) {
                    output.push(ParserEvent::Typst(TypstEvent::Raw("#footnote[".into())));
                    output.extend(body.iter().cloned());
                    output.push(ParserEvent::Typst(TypstEvent::Raw("]".into())));
                } else {
                    output.push(ParserEvent::Markdown(MarkdownEvent::FootnoteReference(
                        label,
                    )));
                }
            }
            event => output.push(event),
        }
    }
    output
}

/// A Markdown footnote definition normally arrives as one paragraph. The
/// Typst converter represents that paragraph as `#par[...]`; nesting it inside
/// `#footnote[...]` makes the book-wide paragraph spacing apply between the
/// footnote rule, marker, and text. Strip only that outer paragraph wrapper.
fn trim_single_paragraph(mut body: Vec<ParserEvent<'_>>) -> Vec<ParserEvent<'_>> {
    let starts_paragraph = matches!(
        body.first(),
        Some(ParserEvent::Typst(TypstEvent::Start(TypstTag::Paragraph)))
    );
    let ends_paragraph = matches!(
        body.last(),
        Some(ParserEvent::Typst(TypstEvent::End(TypstTag::Paragraph)))
    );
    if starts_paragraph && ends_paragraph {
        body.remove(0);
        body.pop();
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pulldown_parses_footnote_definitions_and_references() {
        let events = MarkdownParser::new_ext(
            "A reference[^note].\n\n[^note]: A *formatted* note.",
            MarkdownOptions::ENABLE_FOOTNOTES,
        )
        .collect::<Vec<_>>();

        assert!(events
            .iter()
            .any(|event| matches!(event, MarkdownEvent::FootnoteReference(_))));
        assert!(events.iter().any(|event| matches!(
            event,
            MarkdownEvent::Start(pullup::markdown::Tag::FootnoteDefinition(_))
        )));
    }

    #[test]
    fn replaces_reference_with_structured_typst_body() {
        let events = vec![
            ParserEvent::Markdown(MarkdownEvent::Start(
                pullup::markdown::Tag::FootnoteDefinition("note".into()),
            )),
            ParserEvent::Typst(TypstEvent::Raw("#quote[Quoted]".into())),
            ParserEvent::Markdown(MarkdownEvent::End(
                pullup::markdown::TagEnd::FootnoteDefinition,
            )),
            ParserEvent::Markdown(MarkdownEvent::FootnoteReference("note".into())),
        ];

        let output = process_events(events.into_iter());
        assert!(matches!(
            output[0],
            ParserEvent::Typst(TypstEvent::Raw(ref text)) if text.to_string() == "#footnote["
        ));
        assert!(matches!(
            output[1],
            ParserEvent::Typst(TypstEvent::Raw(ref text)) if text.to_string() == "#quote[Quoted]"
        ));
        assert!(matches!(
            output[2],
            ParserEvent::Typst(TypstEvent::Raw(ref text)) if text.to_string() == "]"
        ));
    }

    #[test]
    fn removes_only_the_outer_single_paragraph() {
        let body = vec![
            ParserEvent::Typst(TypstEvent::Start(TypstTag::Paragraph)),
            ParserEvent::Typst(TypstEvent::Raw("text".into())),
            ParserEvent::Typst(TypstEvent::End(TypstTag::Paragraph)),
        ];
        let body = trim_single_paragraph(body);
        assert_eq!(body.len(), 1);
        assert!(matches!(body[0], ParserEvent::Typst(TypstEvent::Raw(_))));
    }
}

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::iter;

use mdbook::renderer::RenderContext;
use pullup::markdown::CowStr;
use pullup::markdown::Event as MdEvent;
use pullup::typst::to::markup::TypstMarkup;
use pullup::typst::TypstFilter;
use pullup::ParserEvent;

mod config;
mod converters;
mod css;
mod footnotes;

use config::Config;
use converters::{
    ConvertCallouts, ConvertHtmlAnchors, CopyReferencedAssets, FixCodeBlockFence,
    FixHeadingStutter, PartToCoverPage, ResolveCssImageStyles,
};
use css::CssClassStyles;

fn none_on_empty(x: &str) -> Option<String> {
    if x.is_empty() {
        None
    } else {
        Some(x.to_string())
    }
}

fn none_on_empty_vec<T: Clone>(x: &[T]) -> Option<Vec<T>> {
    if x.is_empty() {
        None
    } else {
        Some(x.to_vec())
    }
}

const TYPST_MARKUP_NAME: &str = "book.typst";

/// Translate an `mdbook` 0.5+ `RenderContext` JSON payload into the shape the
/// `mdbook` 0.4 crate expects, so this backend keeps working with both major
/// versions of the `mdbook` CLI.
///
/// Two incompatibilities are handled:
///
/// 1. `Book::sections` was renamed to `Book::items` in mdBook 0.5
///    (rust-lang/mdBook#2813). 0.4's `Book` also has a private
///    `__non_exhaustive: ()` field that must appear in the JSON.
/// 2. mdBook 0.4 deserializes its `Config` via `toml::Value`, which has no
///    null. mdBook 0.5 happily emits `null` for unset optional fields, so we
///    strip nulls from the config subtree.
fn translate_render_context_json(input: &str) -> String {
    use serde_json::Value;

    let Ok(mut value) = serde_json::from_str::<Value>(input) else {
        return input.to_string();
    };

    if let Some(book) = value.get_mut("book").and_then(Value::as_object_mut) {
        if let Some(items) = book.remove("items") {
            book.entry("sections").or_insert(items);
        }
        book.entry("__non_exhaustive").or_insert(Value::Null);
    }

    if let Some(config) = value.get_mut("config") {
        strip_nulls(config);
    }

    serde_json::to_string(&value).unwrap_or_else(|_| input.to_string())
}

fn strip_nulls(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.retain(|_, v| !v.is_null());
            map.values_mut().for_each(strip_nulls);
        }
        serde_json::Value::Array(arr) => arr.iter_mut().for_each(strip_nulls),
        _ => {}
    }
}

fn main() -> Result<(), std::io::Error> {
    tracing_subscriber::fmt().init();

    let mut raw = String::new();
    io::stdin().read_to_string(&mut raw)?;
    let translated = translate_render_context_json(&raw);
    let ctx = RenderContext::from_json(translated.as_bytes()).unwrap();
    let cfg: Config = ctx
        .config
        .get_deserialized_opt("output.typst")
        .expect("output.typst config")
        .unwrap_or_default();

    let horizontal_rule_replacement: String;

    // Load CSS styles from the book's source directory.
    let mut css_styles = CssClassStyles::new();
    css_styles.load_css_directory(&ctx.source_dir());
    // Also load CSS from the root (where ferris.css typically lives)
    css_styles.load_css_directory(&ctx.root);
    let css_styles = std::sync::Arc::new(css_styles);

    // Parse mdBook content with pulldown-cmark footnotes enabled.
    let parser = footnotes::mdbook_events(&ctx);

    // Convert the mdbook events to pullup `ParserEvent`s.
    let mut events: Box<dyn Iterator<Item = ParserEvent<'_>>> = Box::new(
        pullup::mdbook::to::typst::Conversion::builder()
            .events(parser.into_iter())
            .build(),
    );

    // Resolve structured footnote definitions after Markdown has been
    // converted to Typst events, retaining the complete event body.
    events = Box::new(footnotes::process_events(events).into_iter());

    //println!("{:#?}", events.collect::<Vec<_>>());
    //panic!("x");

    // Run some special converters.
    events = Box::new(PartToCoverPage::new(events));

    // Fix heading stutter (duplicate chapter title + first heading).
    events = Box::new(FixHeadingStutter::new(events));

    // Fix mdBook code block fence names (e.g., "rust,ignore" -> "rust")
    // and handle filename attributes.
    events = Box::new(FixCodeBlockFence::new(events));

    // Convert blockquotes starting with "Note:", "Warning:", etc. to styled callouts.
    events = Box::new(ConvertCallouts::new(events));

    // Copy referenced assets (images) to the destination directory.
    // This must come BEFORE ConvertHtmlAnchors so it can see HTML img events.
    events = Box::new(CopyReferencedAssets::new(
        events,
        ctx.source_dir(),
        ctx.destination.clone(),
    ));

    // Convert HTML anchor elements (<a id="...">) and img tags to Typst.
    events = Box::new(ConvertHtmlAnchors::new(events, css_styles.clone()));

    // Resolve CSS classes on Image events to actual width/height values.
    events = Box::new(ResolveCssImageStyles::new(events, css_styles.clone()));

    // Figure out the output filename and location.
    let outname = if let Some(n) = cfg.output.name {
        use config::OutputFormat;

        let numbered = n.contains("{n}");
        if !numbered && matches!(cfg.output.format, OutputFormat::Png | OutputFormat::Svg) {
            eprintln!("cannot export images without `{{n}}` in output path");
            std::process::exit(-1);
        }
        n
    } else {
        use config::OutputFormat::*;
        match cfg.output.format {
            Pdf => "book.pdf".to_string(),
            Svg => "book{n}.svg".to_string(),
            Png => "book{n}.png".to_string(),
            Typst => TYPST_MARKUP_NAME.to_string(),
        }
    };

    let _ = fs::create_dir_all(&ctx.destination);

    let markup_path = ctx.destination.join(TYPST_MARKUP_NAME);
    let final_path = ctx.destination.join(outname.clone());

    // -------- Styles --------
    // Note that if cfg item is not set we use default style, and if set
    // to empty string we don't include that style at all.
    let mut style_events = vec![];

    // Paper size.
    if let Some(paper) = cfg
        .style
        .paper
        .as_ref()
        .map_or(Some(config::default_paper()), |s| none_on_empty(s))
    {
        style_events.push(pullup::ParserEvent::Typst(pullup::typst::Event::Set(
            "page".into(),
            "paper".into(),
            format!("\"{}\"", paper).into(),
        )));
    }

    // Text size.
    if let Some(text_size) = cfg
        .style
        .text_size
        .as_ref()
        .map_or(Some(config::default_text_size()), |s| none_on_empty(s))
    {
        style_events.push(pullup::ParserEvent::Typst(pullup::typst::Event::Set(
            "text".into(),
            "size".into(),
            text_size.into(),
        )));
    }

    // Text font.
    if let Some(text_font) = cfg
        .style
        .text_font
        .as_ref()
        .map_or(Some(config::default_text_font()), |s| none_on_empty(s))
    {
        style_events.push(pullup::ParserEvent::Typst(pullup::typst::Event::Set(
            "text".into(),
            "font".into(),
            format!("\"{}\"", text_font).into(),
        )));
    }

    // Paragraph spacing.
    if let Some(paragraph_spacing) = cfg
        .style
        .paragraph_spacing
        .as_ref()
        .map_or(Some(config::default_paragraph_spacing()), |s| {
            none_on_empty(s)
        })
    {
        style_events.push(pullup::ParserEvent::Typst(pullup::typst::Event::Set(
            "par".into(),
            "spacing".into(),
            paragraph_spacing.into(),
        )));
    }

    // Paragraph leading.
    if let Some(paragraph_leading) = cfg
        .style
        .paragraph_leading
        .as_ref()
        .map_or(Some(config::default_paragraph_leading()), |s| {
            none_on_empty(s)
        })
    {
        style_events.push(pullup::ParserEvent::Typst(pullup::typst::Event::Set(
            "par".into(),
            "leading".into(),
            paragraph_leading.into(),
        )));
    }

    // Heading numbering.
    // Note this is a bit different as we don't set a default.
    if let Some(heading_numbering) = cfg
        .style
        .heading_numbering
        .as_ref()
        .and_then(|s| none_on_empty(s))
    {
        style_events.push(pullup::ParserEvent::Typst(pullup::typst::Event::Set(
            "heading".into(),
            "numbering".into(),
            format!("\"{}\"", heading_numbering).into(),
        )));
    }

    // Heading above/below. Must be emitted together.
    // TODO: strongly type the event.
    let heading_above = cfg
        .style
        .heading_above
        .as_ref()
        .map_or(Some(config::default_heading_above()), |s| none_on_empty(s));
    let heading_below = cfg
        .style
        .heading_below
        .as_ref()
        .map_or(Some(config::default_heading_below()), |s| none_on_empty(s));
    match (heading_above, heading_below) {
        (None, None) => (),
        (None, Some(below)) => {
            style_events.push(pullup::ParserEvent::Typst(pullup::typst::Event::Raw(
                format!(
                    "
        #show heading: it => [
            #block(below: {}, it)
        ]\n",
                    below
                )
                .into(),
            )));
        }
        (Some(above), None) => {
            style_events.push(pullup::ParserEvent::Typst(pullup::typst::Event::Raw(
                format!(
                    "
        #show heading: it => [
            #block(above: {}, it)
        ]\n",
                    above
                )
                .into(),
            )));
        }
        (Some(above), Some(below)) => {
            style_events.push(pullup::ParserEvent::Typst(pullup::typst::Event::Raw(
                format!(
                    "
        #show heading: it => [
            #block(above: {}, below: {}, it)
        ]\n",
                    above, below,
                )
                .into(),
            )));
        }
    }

    // Link underline.
    // TODO: strongly type the event.
    if cfg
        .style
        .link_underline
        .unwrap_or_else(|| config::default_link_underline().expect("a value"))
    {
        style_events.push(pullup::ParserEvent::Typst(pullup::typst::Event::Raw(
            "#show link: underline\n".into(),
        )));
    }

    // Link color.
    // TODO: strongly type the event.
    if let Some(link_color) = cfg
        .style
        .link_color
        .as_ref()
        .map_or(Some(config::default_link_color()), |s| none_on_empty(s))
    {
        style_events.push(pullup::ParserEvent::Typst(pullup::typst::Event::Raw(
            format!("#show link: set text({})\n", link_color).into(),
        )));
    }

    // -------- Replacements --------
    // Note that if cfg item is not set we use default, and if set
    // to empty string we don't include the output at all.

    // Horizontal rule replacement. This is required as Typst doesn't have the concept
    // of a horizontal rule (unlike Markdown and HTML).
    if let Some(rule) = cfg
        .markup
        .horizontal_rule
        .as_ref()
        .map_or(Some(config::default_markup_horizontal_rule()), |s| {
            none_on_empty(s)
        })
    {
        horizontal_rule_replacement = rule;
        events = Box::new(events.map(|e| match e {
            pullup::ParserEvent::Markdown(MdEvent::Rule) => pullup::ParserEvent::Typst(
                pullup::typst::Event::Raw(horizontal_rule_replacement.clone().into()),
            ),
            _ => e,
        }));
    }

    // -------- Table of Contents / Outline --------
    let mut toc_events = vec![];

    // Toc.
    if cfg
        .toc
        .enable
        .unwrap_or_else(|| config::default_toc_enable().expect("a value"))
    {
        // Show rules.
        if let Some(show_rules) = cfg
            .toc
            .entry_show_rules
            .as_ref()
            .map_or(config::default_toc_entry_show_rules(), |v| {
                none_on_empty_vec(v)
            })
        {
            toc_events.extend(show_rules.into_iter().flat_map(|x| {
                let it = if x.strong.unwrap() {
                    "strong(it)"
                } else {
                    "it"
                };
                let tag = pullup::typst::Tag::Show(
                    pullup::typst::ShowType::Function,
                    format!("outline.entry.where(level: {})", x.level.unwrap()).into(),
                    None,
                    Some(format!("it => block(above: {})[#{}]", x.text_size.unwrap(), it).into()),
                );
                vec![
                    pullup::ParserEvent::Typst(pullup::typst::Event::Start(tag.clone())),
                    pullup::ParserEvent::Typst(pullup::typst::Event::End(tag)),
                ]
            }));
        }

        let mut args: Vec<CowStr<'_>> = vec![];

        // Depth.
        if let Some(depth) = cfg
            .toc
            .depth
            .as_ref()
            .or(Some(&config::default_toc_depth()))
        {
            args.push(format!("depth: {}", depth).into())
        }

        // Indent.
        if let Some(indent) = cfg
            .toc
            .indent
            .as_ref()
            .map_or(Some(config::default_toc_indent()), |s| none_on_empty(s))
        {
            args.push(format!("indent: {}", indent).into())
        }
        toc_events.push(pullup::ParserEvent::Typst(
            pullup::typst::Event::FunctionCall(None, "outline".into(), args),
        ));
        toc_events.push(pullup::ParserEvent::Typst(pullup::typst::Event::PageBreak));
    }

    if !cfg
        .style
        .enable
        .unwrap_or_else(|| config::default_style_enable().expect("a value"))
    {
        style_events.clear();
    }

    // Optional user template. The template is inserted after the generated
    // document-level `#set` statements, matching the original fork workflow.
    let template = if cfg
        .template
        .enable
        .unwrap_or_else(|| config::default_template_enable().expect("a value"))
    {
        let name = cfg
            .template
            .name
            .as_ref()
            .map_or(Some(config::default_template_name()), |s| none_on_empty(s));
        let arg = cfg
            .template
            .arg
            .as_ref()
            .map_or(Some(config::default_template_arg()), |s| none_on_empty(s));
        match (name, arg) {
            (Some(name), Some(arg)) => format!(
                "#import \"{}\": template\n#show: template.with(\n{})\n",
                name, arg
            ),
            _ => String::new(),
        }
    } else {
        String::new()
    };

    // Aggregate synthesized events in proper order.
    events = Box::new(style_events.into_iter().chain(toc_events).chain(events));

    // -------- Escape Hatches --------

    // Prepend the raw Typist markup header if we have one.
    if let Some(header) = cfg.advanced.typst_markup_header {
        events = Box::new(
            iter::once(pullup::ParserEvent::Typst(pullup::typst::Event::Raw(
                header.into(),
            )))
            .chain(events),
        );
    }

    // Append the raw Typst markup footer if we have one.
    if let Some(footer) = cfg.advanced.typst_markup_footer {
        events = Box::new(events.chain(iter::once(pullup::ParserEvent::Typst(
            pullup::typst::Event::Raw(footer.into()),
        ))));
    }

    // -------- Output --------

    // Filter out non-Typst pullup events.
    let events = TypstFilter(events);

    // Bubble up events that must be output first in Typst markup.
    // TODO: use `partition_in_place` when stable.
    let (front, back): (Vec<_>, Vec<_>) = events.partition(|x| {
        matches!(
            x,
            pullup::typst::Event::DocumentFunctionCall(_) | pullup::typst::Event::DocumentSet(_, _)
        )
    });
    let events = front.into_iter().chain(back);

    // Convert the events to Typst markup.
    let markup = TypstMarkup::new(events);

    // Collect all markup into a string first
    let full_markup: String = markup.collect();

    // Post-process HTML comments embedded in table cells to convert them to images
    let full_markup =
        converters::process_html_comments(&full_markup, &css_styles).replace(r"====", "===");

    let full_markup = if template.is_empty() {
        full_markup
    } else {
        let mut output = String::new();
        let mut inserted = false;
        for line in full_markup.split_inclusive('\n') {
            if !inserted && !line.trim_start().starts_with("#set") {
                output.push_str(&template);
                inserted = true;
            }
            output.push_str(line);
        }
        if !inserted {
            output.push_str(&template);
        }
        output
    };

    // Write the Typst markup to filesystem.
    let mut f = File::create(&markup_path).unwrap();
    write!(f, "{}", full_markup)?;

    // Command to use to call the `typst` binary for further processing if required.
    // TODO: use the `typst` library directly.
    let command = match cfg.output.format {
        config::OutputFormat::Pdf => {
            let mut c = std::process::Command::new("typst");
            c.arg("compile")
                .arg("--format")
                .arg("pdf")
                .arg(&markup_path)
                .arg(&final_path);
            Some(c)
        }
        config::OutputFormat::Svg => {
            let mut c = std::process::Command::new("typst");
            c.arg("compile")
                .arg("--format")
                .arg("svg")
                .arg(&markup_path)
                .arg(&final_path);
            Some(c)
        }
        config::OutputFormat::Png => {
            let mut c = std::process::Command::new("typst");
            c.arg("compile")
                .arg("--format")
                .arg("png")
                .arg(&markup_path)
                .arg(&final_path);
            Some(c)
        }
        config::OutputFormat::Typst => None,
    };

    if let Some(mut c) = command {
        let output = c.output().unwrap();
        io::stdout().write_all(&output.stdout).unwrap();
        io::stderr().write_all(&output.stderr).unwrap();
        if !output.status.success() {
            std::process::exit(-2);
        }
        io::stderr().write_all(&output.stderr).unwrap();
    } else if markup_path != final_path {
        std::fs::rename(markup_path, final_path)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx_05() -> serde_json::Value {
        json!({
            "version": "0.5.2",
            "root": "/tmp/book",
            "book": {
                "items": [{
                    "Chapter": {
                        "name": "Chapter 1",
                        "content": "# Chapter 1\n",
                        "number": [1],
                        "sub_items": [],
                        "path": "chapter_1.md",
                        "source_path": "chapter_1.md",
                        "parent_names": []
                    }
                }]
            },
            "config": {
                "book": {
                    "title": "Test",
                    "authors": ["Christian Legnitto"],
                    "description": null,
                    "language": "en",
                    "text-direction": null
                },
                "output": { "typst": { "command": "mdbook-typst" } }
            },
            "destination": "/tmp/book/book"
        })
    }

    fn ctx_04() -> serde_json::Value {
        json!({
            "version": "0.4.52",
            "root": "/tmp/book",
            "book": {
                "sections": [{
                    "Chapter": {
                        "name": "Chapter 1",
                        "content": "# Chapter 1\n",
                        "number": [1],
                        "sub_items": [],
                        "path": "chapter_1.md",
                        "source_path": "chapter_1.md",
                        "parent_names": []
                    }
                }],
                "__non_exhaustive": null
            },
            "config": {
                "book": {
                    "title": "Test",
                    "authors": ["Christian Legnitto"],
                    "src": "src",
                    "language": "en"
                },
                "output": { "typst": { "command": "mdbook-typst" } }
            },
            "destination": "/tmp/book/book"
        })
    }

    #[test]
    fn translates_mdbook_05_payload() {
        let translated = translate_render_context_json(&ctx_05().to_string());
        let value: serde_json::Value = serde_json::from_str(&translated).unwrap();

        let book = value["book"].as_object().unwrap();
        assert!(
            book.contains_key("sections"),
            "items should be renamed to sections"
        );
        assert!(!book.contains_key("items"));
        assert!(book.contains_key("__non_exhaustive"));

        let book_cfg = value["config"]["book"].as_object().unwrap();
        assert!(
            !book_cfg.contains_key("description"),
            "null fields should be stripped"
        );
        assert!(!book_cfg.contains_key("text-direction"));
        assert_eq!(book_cfg["title"], "Test");

        // The translated payload must be deserializable by mdbook 0.4.
        RenderContext::from_json(translated.as_bytes())
            .expect("0.5 payload deserializes after translation");
    }

    #[test]
    fn passes_through_mdbook_04_payload() {
        let original = ctx_04().to_string();
        let translated = translate_render_context_json(&original);

        // Existing 0.4 fields are preserved (not double-renamed or stripped).
        let value: serde_json::Value = serde_json::from_str(&translated).unwrap();
        assert!(value["book"]["sections"].is_array());
        assert!(value["book"].get("items").is_none());
        assert_eq!(value["config"]["book"]["title"], "Test");

        RenderContext::from_json(translated.as_bytes()).expect("0.4 payload deserializes");
    }

    #[test]
    fn invalid_json_is_returned_unchanged() {
        let translated = translate_render_context_json("not json {");
        assert_eq!(translated, "not json {");
    }

    #[test]
    fn strip_nulls_is_recursive() {
        let mut value = json!({
            "a": null,
            "b": { "c": null, "d": 1, "e": { "f": null } },
            "g": [{ "h": null, "i": 2 }]
        });
        strip_nulls(&mut value);
        assert_eq!(
            value,
            json!({
                "b": { "d": 1, "e": {} },
                "g": [{ "i": 2 }]
            })
        );
    }
}

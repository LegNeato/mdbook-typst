use std::fs::{self, File};
use std::io::{self, Write};
use std::iter;

use mdbook::renderer::RenderContext;
use pullup::markdown::CowStr;
use pullup::typst::to::markup::TypstMarkup;
use pullup::typst::TypstFilter;
use pullup::ParserEvent;

mod config;
mod converters;

use config::Config;
use converters::{FixHeadingStutter, PartToCoverPage};

fn none_on_empty(x: &String) -> Option<String> {
    if x.is_empty() {
        None
    } else {
        Some(x.clone())
    }
}

fn none_on_empty_vec<T: Clone>(x: &Vec<T>) -> Option<Vec<T>> {
    if x.is_empty() {
        None
    } else {
        Some(x.clone())
    }
}

const TYPST_MARKUP_NAME: &str = "book.typst";

fn main() -> Result<(), std::io::Error> {
    tracing_subscriber::fmt().init();

    let mut stdin = io::stdin();
    let ctx = RenderContext::from_json(&mut stdin).unwrap();
    let cfg: Config = ctx
        .config
        .get_deserialized_opt("output.typst")
        .expect("output.typst config")
        .unwrap_or_default();

    // Parse mdbook to events.
    let parser = pullup::mdbook::Parser::from_rendercontext(&ctx);

    // Convert the mdbook events to pullup `ParserEvent`s.
    let mut events: Box<dyn Iterator<Item = ParserEvent<'_>>> = Box::new(
        pullup::mdbook::to::typst::Conversion::builder()
            .events(parser.iter().cloned())
            .build(),
    );

    //println!("{:#?}", events.collect::<Vec<_>>());
    //panic!("x");

    // Run some special converters.
    events = Box::new(PartToCoverPage::new(events));

    events = Box::new(FixHeadingStutter::new(events));

    // Inside your event processing loop:

    events = Box::new(events.map(|event| match event {
        // Detect horizontal rule (hr) in markdown.
        ParserEvent::Markdown(pullup::markdown::Event::Rule) => {
            pullup::ParserEvent::Typst(pullup::typst::Event::Raw(
                //"#line(length: 100%)\n".into(),
                "#align(center, line(length: 60%))\n".into(),
            ))
        }
        // Keep all other events unchanged.
        _ => event,
    }));
    println!("hello there!");

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
        .map_or(Some(config::default_paper()), none_on_empty)
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
        .map_or(Some(config::default_text_size()), none_on_empty)
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
        .map_or(Some(config::default_text_font()), none_on_empty)
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
        .map_or(Some(config::default_paragraph_spacing()), none_on_empty)
    {
        let tag = pullup::typst::Tag::Show(
            pullup::typst::ShowType::ShowSet,
            "par".into(),
            Some(("block".into(), "spacing".into(), paragraph_spacing.into())),
            None,
        );
        style_events.push(pullup::ParserEvent::Typst(pullup::typst::Event::Start(
            tag.clone(),
        )));
        style_events.push(pullup::ParserEvent::Typst(pullup::typst::Event::End(tag)));
    }

    // Paragraph leading.
    if let Some(paragraph_leading) = cfg
        .style
        .paragraph_leading
        .as_ref()
        .map_or(Some(config::default_paragraph_leading()), none_on_empty)
    {
        style_events.push(pullup::ParserEvent::Typst(pullup::typst::Event::Set(
            "par".into(),
            "leading".into(),
            paragraph_leading.into(),
        )));
    }

    // Heading numbering.
    // Note this is a bit different as we don't set a default.
    if let Some(heading_numbering) = cfg.style.heading_numbering.as_ref().and_then(none_on_empty) {
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
        .map_or(Some(config::default_heading_above()), none_on_empty);
    let heading_below = cfg
        .style
        .heading_below
        .as_ref()
        .map_or(Some(config::default_heading_below()), none_on_empty);
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
        .map_or(Some(config::default_link_color()), none_on_empty)
    {
        style_events.push(pullup::ParserEvent::Typst(pullup::typst::Event::Raw(
            format!("#show link: set text({})\n", link_color).into(),
        )));
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
            .map_or(config::default_toc_entry_show_rules(), none_on_empty_vec)
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
                    Some(
                        format!(
                            "it => {{
                                    v({}, weak: true)
                                   {} 
                                }}",
                            x.text_size.unwrap(),
                            it
                        )
                        .into(),
                    ),
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
            .map_or(Some(config::default_toc_indent()), none_on_empty)
        {
            args.push(format!("indent: {}", indent).into())
        }
        toc_events.push(pullup::ParserEvent::Typst(
            pullup::typst::Event::FunctionCall(None, "outline".into(), args),
        ));
        toc_events.push(pullup::ParserEvent::Typst(pullup::typst::Event::PageBreak));
    }

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

    // Write the Typst markup to filesystem.
    let mut f = File::create(&markup_path).unwrap();
    for m in markup {
        write!(f, "{}", m)?;
    }

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

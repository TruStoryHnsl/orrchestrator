use image::{ImageBuffer, Rgba, DynamicImage};
use imageproc::drawing::draw_text_mut;
use ab_glyph::{FontRef, PxScale};
use pulldown_cmark::{Parser, Event, Tag, TagEnd};

pub fn render_markdown_to_image(md: &str) -> DynamicImage {
    // Load fonts
    let font_regular_data = std::fs::read("/usr/share/fonts/Adwaita/AdwaitaSans-Regular.ttf").unwrap_or_else(|_| vec![]);
    let font_regular = FontRef::try_from_slice(&font_regular_data).unwrap();
    
    let font_bold_data = std::fs::read("/usr/share/fonts/Adwaita/AdwaitaMono-Bold.ttf").unwrap_or_else(|_| vec![]);
    let font_bold = FontRef::try_from_slice(&font_bold_data).unwrap_or(font_regular.clone());

    let font_mono_data = std::fs::read("/usr/share/fonts/Adwaita/AdwaitaMono-Regular.ttf").unwrap_or_else(|_| vec![]);
    let font_mono = FontRef::try_from_slice(&font_mono_data).unwrap_or(font_regular.clone());

    // Assume fixed width, ratatui-image will scale it.
    let width: i32 = 1200;
    
    // First pass to find required height
    let mut x = 20;
    let mut y = 20;
    
    let parser = Parser::new(md);
    let mut current_font = &font_regular;
    let mut current_scale = PxScale::from(24.0);
    let mut line_height = 28;

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                current_font = &font_bold;
                current_scale = match level {
                    pulldown_cmark::HeadingLevel::H1 => PxScale::from(40.0),
                    pulldown_cmark::HeadingLevel::H2 => PxScale::from(36.0),
                    pulldown_cmark::HeadingLevel::H3 => PxScale::from(30.0),
                    _ => PxScale::from(24.0),
                };
                line_height = current_scale.y as i32 + 10;
                x = 20;
            }
            Event::End(TagEnd::Heading(_)) => {
                current_font = &font_regular;
                current_scale = PxScale::from(24.0);
                line_height = 28;
                x = 20;
                y += line_height;
            }
            Event::Start(Tag::Paragraph) => { x = 20; }
            Event::End(TagEnd::Paragraph) => { x = 20; y += line_height + 10; }
            Event::Start(Tag::Strong) => { current_font = &font_bold; }
            Event::End(TagEnd::Strong) => { current_font = &font_regular; }
            Event::Start(Tag::CodeBlock(_)) => { current_font = &font_mono; x = 30; }
            Event::End(TagEnd::CodeBlock) => { current_font = &font_regular; x = 20; y += line_height; }
            Event::Start(Tag::Item) => { x = 40; }
            Event::End(TagEnd::Item) => { x = 20; y += line_height; }
            Event::Text(text) => {
                // simple word wrap simulation
                let w = imageproc::drawing::text_size(current_scale, current_font, &text).0 as i32;
                if x + w > width - 20 {
                    x = 40; // indent wrapped line
                    y += line_height;
                }
                x += w;
            }
            Event::SoftBreak | Event::HardBreak => { x = 20; y += line_height; }
            _ => {}
        }
    }
    
    let height = (y + 40) as u32;
    let mut img = ImageBuffer::from_pixel(width as u32, height, Rgba([30u8, 30u8, 40u8, 255u8]));
    let color = Rgba([230u8, 230u8, 240u8, 255u8]);

    // Second pass to draw
    x = 20;
    y = 20;
    current_font = &font_regular;
    current_scale = PxScale::from(24.0);
    line_height = 28;
    
    let parser = Parser::new(md);
    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                current_font = &font_bold;
                current_scale = match level {
                    pulldown_cmark::HeadingLevel::H1 => PxScale::from(40.0),
                    pulldown_cmark::HeadingLevel::H2 => PxScale::from(36.0),
                    pulldown_cmark::HeadingLevel::H3 => PxScale::from(30.0),
                    _ => PxScale::from(24.0),
                };
                line_height = current_scale.y as i32 + 10;
                x = 20;
            }
            Event::End(TagEnd::Heading(_)) => {
                current_font = &font_regular;
                current_scale = PxScale::from(24.0);
                line_height = 28;
                x = 20;
                y += line_height;
            }
            Event::Start(Tag::Paragraph) => { x = 20; }
            Event::End(TagEnd::Paragraph) => { x = 20; y += line_height + 10; }
            Event::Start(Tag::Strong) => { current_font = &font_bold; }
            Event::End(TagEnd::Strong) => { current_font = &font_regular; }
            Event::Start(Tag::CodeBlock(_)) => { current_font = &font_mono; x = 30; }
            Event::End(TagEnd::CodeBlock) => { current_font = &font_regular; x = 20; y += line_height; }
            Event::Start(Tag::Item) => {
                x = 40;
                draw_text_mut(&mut img, color, 20, y, current_scale, current_font, "•");
            }
            Event::End(TagEnd::Item) => { x = 20; y += line_height; }
            Event::Text(text) => {
                // simple word wrap simulation
                let words: Vec<&str> = text.split_whitespace().collect();
                for word in words {
                    let w = imageproc::drawing::text_size(current_scale, current_font, word).0 as i32;
                    let space_w = imageproc::drawing::text_size(current_scale, current_font, " ").0 as i32;
                    if x + w > width - 20 {
                        x = 40;
                        y += line_height;
                    }
                    draw_text_mut(&mut img, color, x, y, current_scale, current_font, word);
                    x += w + space_w;
                }
            }
            Event::SoftBreak | Event::HardBreak => { x = 20; y += line_height; }
            _ => {}
        }
    }

    DynamicImage::ImageRgba8(img)
}

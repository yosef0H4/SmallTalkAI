//! Wayland-free rendering for the Linux overlay.
//!
//! Sizes the translucent tooltip to the same Pango text layout it draws.

use crate::localization::Locale;

use super::overlay_font::{ensure_registered, ICON_FAMILY};
use super::overlay_model::{
    render_overlay_text_with_locale, OverlayPrimary, RecordingIndicators, VisualizerLevels,
};

// Monospace font that renders the block glyphs (▁▂▃…▇) full-cell and crisp,
// matching how they look in a terminal. JetBrainsMono is SIL OFL licensed.
// The fallback chain keeps it working if that family is not installed.
pub const FONT: &str = "JetBrainsMono Nerd Font, JetBrains Mono, monospace 12";

// Box / content layout (logical pixels).
const PAD_X: i32 = 12;
const PAD_Y: i32 = 8;
const LINE_SPACING_PX: i32 = 2;
const PANEL_ALPHA: f64 = 0.84;

pub const MIN_WIDTH: u32 = 90;
pub const MIN_HEIGHT: u32 = 40;
pub const MAX_WIDTH: u32 = 560;
pub const MAX_HEIGHT: u32 = 420;

pub struct LayoutPlan {
    pub locale: Locale,
    dark: bool,
    pub width: u32,
    pub height: u32,
    pub text: String,
    pub signature: String,
}

/// Build the overlay text and the surface size needed to fit it.
pub fn plan_for(
    primary: OverlayPrimary,
    notice: Option<&str>,
    levels: &VisualizerLevels,
    indicators: RecordingIndicators,
) -> Option<LayoutPlan> {
    plan_for_locale(primary, notice, levels, indicators, Locale::En)
}

pub fn plan_for_locale(
    primary: OverlayPrimary,
    notice: Option<&str>,
    levels: &VisualizerLevels,
    indicators: RecordingIndicators,
    locale: Locale,
) -> Option<LayoutPlan> {
    plan_with_shortcuts(primary, notice, levels, indicators, locale, "", false)
}

pub fn plan_with_shortcuts(
    primary: OverlayPrimary,
    notice: Option<&str>,
    levels: &VisualizerLevels,
    indicators: RecordingIndicators,
    locale: Locale,
    shortcuts: &str,
    enabled: bool,
) -> Option<LayoutPlan> {
    let text = crate::shortcuts::append(
        render_overlay_text_with_locale(primary, notice, levels, indicators, locale),
        shortcuts,
        enabled,
    );
    plan_text(text, locale, false)
}

pub(super) fn plan_text(text: String, locale: Locale, dark: bool) -> Option<LayoutPlan> {
    if text.trim().is_empty() {
        return None;
    }

    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1).ok()?;
    let cr = cairo::Context::new(&surface).ok()?;
    let layout = build_layout(&cr, &text, locale, MAX_WIDTH as i32 - 2 * PAD_X);
    let (w, h) = layout.pixel_size();

    let width = ((w + 2 * PAD_X) as u32).clamp(MIN_WIDTH, MAX_WIDTH);
    let height = ((h + 2 * PAD_Y) as u32).clamp(MIN_HEIGHT, MAX_HEIGHT);
    Some(LayoutPlan {
        width,
        height,
        signature: format!("{}:{dark}:{text}", locale.as_str()),
        locale,
        text,
        dark,
    })
}

/// Render a plan to an ARGB32 Cairo image surface of `plan.width × plan.height`.
pub fn render_surface(plan: &LayoutPlan) -> Option<cairo::ImageSurface> {
    let width = plan.width as i32;
    let height = plan.height as i32;
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height).ok()?;
    let cr = cairo::Context::new(&surface).ok()?;

    // Leave the screen-facing edge open on the top-right anchored tooltip.
    let (w, h) = (f64::from(width), f64::from(height));
    let (outline, fill, highlight, ink) = if plan.dark {
        (0.03, 0.085, 0.49, 0.95)
    } else {
        (0.31, 1.0, 0.93, 0.14)
    };
    cr.set_source_rgb(outline, outline, outline);
    cr.rectangle(0.0, 0.0, w, 1.0);
    cr.rectangle(0.0, h - 1.0, w, 1.0);
    cr.rectangle(0.0, 1.0, 1.0, h - 2.0);
    cr.fill().ok()?;
    cr.set_source_rgba(fill, fill, fill, PANEL_ALPHA);
    cr.rectangle(2.0, 2.0, w - 2.0, h - 4.0);
    cr.fill().ok()?;
    cr.set_source_rgb(highlight, highlight, highlight);
    cr.rectangle(1.0, 1.0, w - 1.0, 1.0);
    cr.rectangle(1.0, h - 2.0, w - 1.0, 1.0);
    cr.rectangle(1.0, 2.0, 1.0, h - 4.0);
    cr.fill().ok()?;

    // Use the same text layout for measurement and drawing.
    let layout = build_layout(&cr, &plan.text, plan.locale, width - 2 * PAD_X);
    cr.move_to(f64::from(PAD_X), f64::from(PAD_Y));
    cr.set_source_rgb(ink, ink, ink);
    pangocairo::functions::show_layout(&cr, &layout);

    drop(layout);
    drop(cr);
    Some(surface)
}

fn build_layout(cr: &cairo::Context, text: &str, locale: Locale, width: i32) -> pango::Layout {
    let bundled_fonts = ensure_registered();
    let layout = pangocairo::functions::create_layout(cr);
    layout.set_text(text);
    // Allow long notices to wrap rather than overflow; the surface is sized to
    // the resulting (possibly smaller) extent.
    layout.set_width(width * pango::SCALE);
    layout.context().set_base_dir(if locale == Locale::Ar {
        pango::Direction::Rtl
    } else {
        pango::Direction::Ltr
    });
    layout
        .context()
        .set_language(Some(&pango::Language::from_string(locale.as_str())));
    layout.set_auto_dir(false);
    layout.set_alignment(if locale == Locale::Ar {
        pango::Alignment::Right
    } else {
        pango::Alignment::Left
    });
    layout.set_wrap(pango::WrapMode::WordChar);
    // Do not ellipsize: notification text must remain readable. Pango wraps it
    // within MAX_WIDTH and plan_for grows the surface vertically. The compact
    // one-line visualizer never reaches this limit, so its layout is unchanged.
    layout.set_ellipsize(pango::EllipsizeMode::None);
    layout.set_spacing(LINE_SPACING_PX * pango::SCALE);
    layout.set_font_description(Some(&pango::FontDescription::from_string(
        if locale == Locale::Ar {
            "Noto Sans Arabic, JetBrains Mono, sans-serif 12"
        } else {
            FONT
        },
    )));
    let attrs = pango::AttrList::new();
    if locale == Locale::Ar {
        let mut start = 0;
        for word in text.split_inclusive('\n') {
            if word.starts_with('\u{2066}') {
                let mut attr = pango::AttrFontDesc::new(&pango::FontDescription::from_string(FONT));
                attr.set_start_index(start);
                attr.set_end_index(start + word.len() as u32);
                attrs.insert(attr);
            }
            start += word.len() as u32;
        }
    }
    decorate_shortcuts(&layout, text, &attrs, bundled_fonts);
    if text.contains('\t') && !shortcut_runs(text).is_empty() {
        let mut widths = [0; 2];
        for line in text.lines().filter(|line| line.contains('\t')) {
            for (column, entry) in line.split('\t').take(2).enumerate() {
                let entry_layout = build_layout(cr, entry, locale, width);
                widths[column] = widths[column].max(entry_layout.pixel_size().0);
            }
        }
        let mut tabs = pango::TabArray::new(2, true);
        tabs.set_tab(0, pango::TabAlign::Left, widths[0] + 20);
        tabs.set_tab(1, pango::TabAlign::Left, widths[0] + widths[1] + 40);
        layout.set_tabs(Some(&tabs));
    }
    layout
}

#[derive(Debug)]
struct ShortcutRun {
    icon: std::ops::Range<usize>,
    keys: std::ops::Range<usize>,
}

// Only app-generated icon + isolated keycap runs receive shortcut styling.
// Notices, transcripts, filenames and ordinary bracketed text stay plain.
fn shortcut_runs(text: &str) -> Vec<ShortcutRun> {
    let mut runs = Vec::new();
    for action in crate::shortcuts::ACTIONS {
        let icon = action.icon;
        let prefix = format!("{icon} \u{2066}[");
        for (start, _) in text.match_indices(&prefix) {
            let keys_start = start + icon.len() + " \u{2066}".len();
            let Some(length) = text[keys_start..].find('\u{2069}') else {
                continue;
            };
            let end = keys_start + length;
            let mut offset = keys_start;
            while offset < end && text.as_bytes()[offset] == b'[' {
                let Some(close) = text[offset + 1..end].find(']') else {
                    break;
                };
                offset += close + 2;
            }
            if keys_start < end && offset == end {
                runs.push(ShortcutRun {
                    icon: start..start + icon.len(),
                    keys: keys_start..end,
                });
            }
        }
    }
    runs
}

fn span(
    attrs: &pango::AttrList,
    attribute: impl Into<pango::Attribute>,
    range: std::ops::Range<usize>,
) {
    let mut attribute = attribute.into();
    attribute.set_start_index(range.start as u32);
    attribute.set_end_index(range.end as u32);
    attrs.insert(attribute);
}

fn decorate_shortcuts(
    layout: &pango::Layout,
    text: &str,
    attrs: &pango::AttrList,
    bundled_fonts: bool,
) {
    let mut icon_font = pango::FontDescription::from_string("emoji 12");
    if bundled_fonts {
        icon_font.set_family(ICON_FAMILY);
        icon_font.set_size(14 * pango::SCALE);
    }
    let style_icon = |range: std::ops::Range<usize>| {
        span(attrs, pango::AttrFontDesc::new(&icon_font), range.clone());
        if bundled_fonts {
            // Keep the shared emoji codepoints while drawing the Linux glyphs.
            span(attrs, pango::AttrInt::new_fallback(false), range);
        }
    };
    for (start, glyph) in text.match_indices("🎙") {
        style_icon(start..start + glyph.len());
    }
    for run in shortcut_runs(text) {
        span(
            attrs,
            pango::AttrFontDesc::new(&pango::FontDescription::from_string("JetBrains Mono 10")),
            run.icon.start..run.keys.end,
        );
        // Whole shortcut groups wrap together; individual keycaps never split.
        span(
            attrs,
            pango::AttrInt::new_allow_breaks(false),
            run.icon.start..run.keys.end,
        );
        style_icon(run.icon);
    }
    layout.set_attributes(Some(attrs));
}

#[cfg(test)]
mod localization_tests {
    use super::*;
    use pango::prelude::FontExt;

    #[test]
    fn both_themes_keep_transparency_and_an_open_right_edge() {
        for dark in [false, true] {
            let plan = plan_text("Next page · ready".into(), Locale::En, dark).unwrap();
            let mut surface = render_surface(&plan).unwrap();
            let stride = surface.stride() as usize;
            let data = surface.data().unwrap();
            let pixel = |x: u32, y: u32| {
                let offset = y as usize * stride + x as usize * 4;
                u32::from_ne_bytes(data[offset..offset + 4].try_into().unwrap())
            };
            let fill = pixel(plan.width / 2, 2);
            let edge = pixel(0, 2);
            assert!((213..=216).contains(&(fill >> 24)));
            let [alpha, red, green, blue] = fill.to_be_bytes();
            assert_eq!([green, blue], [red; 2]);
            if dark {
                assert!(red < 32);
            } else {
                assert_eq!(red, alpha);
            }
            assert_eq!(fill, pixel(plan.width - 1, 2));
            assert_ne!(fill, edge);
            let highlight = pixel(1, 2);
            assert_eq!(highlight >> 24, 255);
            assert_ne!(highlight, edge);
            assert_eq!(highlight, pixel(plan.width / 2, 1));
            assert_eq!(edge, pixel(plan.width / 2, 0));
            assert_eq!(edge, pixel(plan.width / 2, plan.height - 1));
        }
    }

    #[test]
    fn only_shortcut_rows_receive_shortcut_layout() {
        assert!(shortcut_runs("File [Meta][Ctrl][.] — обычный текст").is_empty());
        let text = crate::shortcuts::summary(
            &std::collections::BTreeMap::from([("delivery".to_owned(), "Meta+Ctrl+.".to_owned())]),
            Locale::En,
        );
        let runs = shortcut_runs(&text);
        assert_eq!(runs.len(), 1);
        assert_eq!(
            &text[runs[0].icon.clone()],
            crate::shortcuts::ACTIONS
                .iter()
                .find(|action| action.id == "delivery")
                .unwrap()
                .icon
        );
        assert_eq!(&text[runs[0].keys.clone()], "[Meta][Ctrl][.]");
    }

    #[test]
    fn shortcut_icons_render_without_missing_glyphs_or_clipping() {
        let bindings = crate::shortcuts::ACTIONS
            .iter()
            .map(|action| (action.id.to_owned(), "Meta+Ctrl+Shift+Z".to_owned()))
            .collect();
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1).unwrap();
        let cr = cairo::Context::new(&surface).unwrap();
        for locale in [Locale::En, Locale::Ar] {
            let shortcuts = crate::shortcuts::summary(&bindings, locale);
            let plan = plan_with_shortcuts(
                OverlayPrimary::Recording,
                None,
                &[0.5; 10],
                RecordingIndicators::default(),
                locale,
                &shortcuts,
                true,
            )
            .unwrap();
            let layout = build_layout(&cr, &plan.text, locale, plan.width as i32 - 2 * PAD_X);
            assert_eq!(layout.unknown_glyphs_count(), 0);
            let shortcuts = shortcut_runs(&plan.text);
            let shaped_runs: Vec<_> = layout
                .lines()
                .iter()
                .flat_map(pango::LayoutLine::runs)
                .collect();
            for start in plan
                .text
                .match_indices("🎙")
                .map(|(start, _)| start)
                .chain(shortcuts.iter().map(|run| run.icon.start))
            {
                let shaped_icon = shaped_runs
                    .iter()
                    .map(pango::GlyphItem::item)
                    .find(|item| {
                        (item.offset()..item.offset() + item.length()).contains(&(start as i32))
                    })
                    .expect("shaped icon");
                assert_eq!(
                    shaped_icon.analysis().font().describe().family().as_deref(),
                    Some(ICON_FAMILY)
                );
            }
            for run in shortcuts {
                let start = layout.index_to_pos(run.keys.start as i32);
                let end = layout.index_to_pos((run.keys.end - 1) as i32);
                assert_eq!(start.y(), end.y(), "shortcut split across lines");
                assert!(
                    start.x() >= 0 && end.x() + end.width() <= layout.width(),
                    "shortcut outside tooltip"
                );
            }
            assert!(
                layout.pixel_size().1 + 2 * PAD_Y <= plan.height as i32,
                "shortcut rows clipped"
            );
        }
    }

    #[test]
    fn arabic_font_and_bidi_render_without_missing_glyphs() {
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1).unwrap();
        let cr = cairo::Context::new(&surface).unwrap();
        let layout = build_layout(&cr, "جارٍ تحميل النموذج — model-q8.gguf", Locale::Ar, 516);
        assert_eq!(layout.unknown_glyphs_count(), 0);
        assert_eq!(layout.context().base_dir(), pango::Direction::Rtl);
        let plan = plan_for_locale(
            OverlayPrimary::Transcribing,
            Some("النموذج جاهز"),
            &[0.0; 10],
            RecordingIndicators::default(),
            Locale::Ar,
        )
        .unwrap();
        assert!(plan.text.contains("جارٍ التفريغ"));
        assert!(plan.width <= MAX_WIDTH && plan.height <= MAX_HEIGHT);
        assert!(render_surface(&plan).is_some());
    }

    #[test]
    fn recording_width_stays_constant_across_waveform_heights() {
        for locale in [Locale::En, Locale::Ar] {
            let widths: Vec<_> = (0..=6)
                .map(|step| {
                    plan_for_locale(
                        OverlayPrimary::Recording,
                        None,
                        &[step as f32 / 6.0; 10],
                        RecordingIndicators::default(),
                        locale,
                    )
                    .unwrap()
                    .width
                })
                .collect();
            assert!(widths.iter().all(|width| *width == widths[0]), "{widths:?}");
        }
    }
}

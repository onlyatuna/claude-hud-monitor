//! Qt Style Sheet (QSS) Engine matching `qtbase/src/widgets/styles/qstylesheetstyle.cpp`.
//!
//! Provides cascading style resolution, specificity scoring, and resolved style metrics
//! for widgets, labels, buttons, and progress bars.

use std::sync::Arc;

use qtrs_gui::text::qcssparser::{
    QCssBasicSelector, QCssDeclaration, QCssProperty, QCssStyleSheet, QCssValue,
};
use qtrs_gui::tiny_skia::Color;

/// Fully resolved concrete style properties for a widget or sub-control.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResolvedStyle {
    pub background_color: Option<Color>,
    pub color: Option<Color>,
    pub border_color: Option<Color>,
    pub border_width: Option<f32>,
    pub border_style: Option<String>,
    pub border_radius: Option<f32>,
    pub min_height: Option<i32>,
    pub max_height: Option<i32>,
    pub min_width: Option<i32>,
    pub max_width: Option<i32>,
    pub font_size: Option<f32>,
    pub font_weight: Option<u16>,
    pub font_family: Option<String>,
    pub letter_spacing: Option<f32>,
    pub padding: Option<[f32; 4]>, // [top, right, bottom, left]
    pub margin: Option<[f32; 4]>,
    pub text_align: Option<String>,
}

impl ResolvedStyle {
    /// Merges another resolved style on top of this one (higher priority overrides lower).
    pub fn merge_with(&mut self, other: &ResolvedStyle) {
        if other.background_color.is_some() {
            self.background_color = other.background_color;
        }
        if other.color.is_some() {
            self.color = other.color;
        }
        if other.border_color.is_some() {
            self.border_color = other.border_color;
        }
        if other.border_width.is_some() {
            self.border_width = other.border_width;
        }
        if other.border_style.is_some() {
            self.border_style = other.border_style.clone();
        }
        if other.border_radius.is_some() {
            self.border_radius = other.border_radius;
        }
        if other.min_height.is_some() {
            self.min_height = other.min_height;
        }
        if other.max_height.is_some() {
            self.max_height = other.max_height;
        }
        if other.min_width.is_some() {
            self.min_width = other.min_width;
        }
        if other.max_width.is_some() {
            self.max_width = other.max_width;
        }
        if other.font_size.is_some() {
            self.font_size = other.font_size;
        }
        if other.font_weight.is_some() {
            self.font_weight = other.font_weight;
        }
        if other.font_family.is_some() {
            self.font_family = other.font_family.clone();
        }
        if other.letter_spacing.is_some() {
            self.letter_spacing = other.letter_spacing;
        }
        if other.padding.is_some() {
            self.padding = other.padding;
        }
        if other.margin.is_some() {
            self.margin = other.margin;
        }
        if other.text_align.is_some() {
            self.text_align = other.text_align.clone();
        }
    }
}

/// Context information for querying matching stylesheet rules.
#[derive(Debug, Clone, Default)]
pub struct WidgetStyleContext<'a> {
    pub type_name: &'a str,
    pub object_name: &'a str,
    pub pseudo_states: &'a [&'a str],
    pub sub_control: Option<&'a str>,
    pub attributes: &'a [(&'a str, &'a str)],
}

/// Qt Style Sheet engine instance.
#[derive(Debug, Clone, Default)]
pub struct QStyleSheetStyle {
    sheet: Arc<QCssStyleSheet>,
}

impl QStyleSheetStyle {
    /// Creates a style from an existing parsed AST.
    pub fn new(sheet: Arc<QCssStyleSheet>) -> Self {
        Self { sheet }
    }

    /// Parses a QSS string and builds an engine instance.
    pub fn parse(qss: &str) -> Self {
        Self {
            sheet: Arc::new(QCssStyleSheet::parse(qss)),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.sheet.is_empty()
    }

    /// Resolves matching styles for a widget context.
    pub fn resolve(&self, ctx: &WidgetStyleContext) -> ResolvedStyle {
        let mut matched_decls: Vec<(u32, &QCssDeclaration)> = Vec::new();

        for rule in &self.sheet.rules {
            let mut best_score = None;
            for selector in &rule.selectors {
                if selector_matches(selector, ctx) {
                    let score = selector.specificity();
                    best_score = Some(best_score.map_or(score, |s: u32| s.max(score)));
                }
            }
            if let Some(score) = best_score {
                for decl in &rule.declarations {
                    matched_decls.push((score, decl));
                }
            }
        }

        // Sort by specificity score (lowest to highest, so higher specificity overwrites)
        matched_decls.sort_by_key(|&(score, _)| score);

        let mut resolved = ResolvedStyle::default();
        for (_, decl) in matched_decls {
            apply_declaration(&mut resolved, decl);
        }

        resolved
    }

    /// Cascades a local stylesheet with a fallback (parent/window/application stylesheet).
    pub fn resolve_cascaded(
        local: Option<&QStyleSheetStyle>,
        fallback: Option<&QStyleSheetStyle>,
        ctx: &WidgetStyleContext,
    ) -> ResolvedStyle {
        let mut base = if let Some(fb) = fallback {
            fb.resolve(ctx)
        } else {
            ResolvedStyle::default()
        };

        if let Some(loc) = local {
            let local_style = loc.resolve(ctx);
            base.merge_with(&local_style);
        }

        base
    }
}

/// Checks whether a selector matches a widget context.
fn selector_matches(selector: &QCssBasicSelector, ctx: &WidgetStyleContext) -> bool {
    // 1. Sub-control match
    if selector.sub_control.as_deref() != ctx.sub_control {
        return false;
    }

    // 2. ID match (e.g. `#CentralWidget`)
    if let Some(id) = &selector.id {
        if id != ctx.object_name {
            return false;
        }
    }

    // 3. Type name match (e.g. `QLabel`, `QProgressBar`)
    if let Some(elem) = &selector.element_name {
        if elem != ctx.type_name && elem != "QWidget" {
            return false;
        }
    }

    // 4. Pseudo-states match (all selector pseudo-states must be active in ctx)
    for ps in &selector.pseudo_states {
        if !ctx.pseudo_states.contains(&ps.as_str()) {
            return false;
        }
    }

    // 5. Attributes match (all selector attributes must match)
    for (k, v) in &selector.attributes {
        let mut matched = false;
        for (ck, cv) in ctx.attributes {
            if ck == k && cv == v {
                matched = true;
                break;
            }
        }
        if !matched {
            return false;
        }
    }

    true
}

/// Applies an individual declaration to a `ResolvedStyle`.
fn apply_declaration(style: &mut ResolvedStyle, decl: &QCssDeclaration) {
    match decl.property {
        QCssProperty::BackgroundColor => {
            if let QCssValue::Color(c) = decl.value {
                style.background_color = Some(c);
            }
        }
        QCssProperty::Color => {
            if let QCssValue::Color(c) = decl.value {
                style.color = Some(c);
            }
        }
        QCssProperty::Border => match &decl.value {
            QCssValue::Border(b) => {
                style.border_width = Some(b.width);
                style.border_style = Some(b.style.clone());
                style.border_color = b.color;
            }
            QCssValue::Identifier(s) if s.eq_ignore_ascii_case("none") => {
                style.border_width = Some(0.0);
                style.border_style = Some("none".to_string());
                style.border_color = None;
            }
            _ => {}
        },
        QCssProperty::BorderColor => {
            if let QCssValue::Color(c) = decl.value {
                style.border_color = Some(c);
            }
        }
        QCssProperty::BorderWidth => {
            if let QCssValue::Length(w) = decl.value {
                style.border_width = Some(w);
            }
        }
        QCssProperty::BorderStyle => {
        if let QCssValue::Identifier(s) = &decl.value {
                style.border_style = Some(s.clone());
            }
        }
        QCssProperty::BorderRadius => {
            if let QCssValue::Length(r) = decl.value {
                style.border_radius = Some(r);
            }
        }
        QCssProperty::MinHeight => {
            if let QCssValue::Length(h) = decl.value {
                style.min_height = Some(h.round() as i32);
            }
        }
        QCssProperty::MaxHeight => {
            if let QCssValue::Length(h) = decl.value {
                style.max_height = Some(h.round() as i32);
            }
        }
        QCssProperty::Height => {
            if let QCssValue::Length(h) = decl.value {
                let px = h.round() as i32;
                style.min_height = Some(px);
                style.max_height = Some(px);
            }
        }
        QCssProperty::MinWidth => {
            if let QCssValue::Length(w) = decl.value {
                style.min_width = Some(w.round() as i32);
            }
        }
        QCssProperty::MaxWidth => {
            if let QCssValue::Length(w) = decl.value {
                style.max_width = Some(w.round() as i32);
            }
        }
        QCssProperty::Width => {
            if let QCssValue::Length(w) = decl.value {
                let px = w.round() as i32;
                style.min_width = Some(px);
                style.max_width = Some(px);
            }
        }
        QCssProperty::FontSize => {
            if let QCssValue::Length(s) = decl.value {
                style.font_size = Some(s);
            }
        }
        QCssProperty::FontWeight => match &decl.value {
            QCssValue::Number(n) => {
                style.font_weight = Some(n.round() as u16);
            }
            QCssValue::Identifier(s) => {
                if s.eq_ignore_ascii_case("bold") {
                    style.font_weight = Some(700);
                } else if s.eq_ignore_ascii_case("normal") {
                    style.font_weight = Some(400);
                }
            }
            _ => {}
        },
        QCssProperty::FontFamily => match &decl.value {
            QCssValue::String(s) | QCssValue::Identifier(s) => {
                style.font_family = Some(s.clone());
            }
            _ => {}
        },
        QCssProperty::LetterSpacing => {
            if let QCssValue::Length(ls) = decl.value {
                style.letter_spacing = Some(ls);
            }
        }
        QCssProperty::Padding => {
            if let QCssValue::Edges(e) = decl.value {
                style.padding = Some(e);
            }
        }
        QCssProperty::Margin => {
            if let QCssValue::Edges(e) = decl.value {
                style.margin = Some(e);
            }
        }
        QCssProperty::TextAlign => {
        if let QCssValue::Identifier(s) = &decl.value {
                style.text_align = Some(s.clone());
            }
        }
        _ => {}
    }
}

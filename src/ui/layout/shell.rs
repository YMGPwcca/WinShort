//! Shell for the layout.

use super::chrome::{brand_row_geometry, top_chrome_geometry, BrandRowGeometry};
use super::geometry::Rect;
#[cfg(test)]
use super::legacy::legacy_rows;
use super::model::{Element, ElementId, ElementKind, LayoutContext, SectionLabel, VisualRegion};
use super::onboarding::{add_onboarding, OnboardingStep};
use super::pages::add_page;
use super::search::add_search_results;
use crate::ui::navigation::Page;
use crate::ui::theme::UiTokens;

#[derive(Debug, Clone)]
pub(crate) struct SettingsLayout {
    pub width: f32,
    pub height: f32,
    pub page: Page,
    pub content_clip: Rect,
    pub content_column: Rect,
    pub footer: Rect,
    pub elements: Vec<Element>,
    pub regions: Vec<VisualRegion>,
    pub sections: Vec<SectionLabel>,
    pub max_scroll: f32,
    pub scroll: f32,
    pub nav_width: f32,
    pub brand: BrandRowGeometry,
    pub top_bar: Rect,
    pub search_rect: Rect,
}

impl SettingsLayout {
    /// Compatibility layout used by pure tests that predate the shell.
    #[cfg(test)]
    pub(crate) fn build(width: f32, height: f32, requested_scroll: f32) -> Self {
        let mut layout = Self::shell_base(width.max(610.0), height.max(500.0), Page::Home);
        let mut y = layout.content_clip.y + 20.0;
        for (id, kind, label, description) in legacy_rows() {
            layout.elements.push(Element {
                id,
                kind,
                rect: Rect::new(layout.content_column.x, y, layout.content_column.w, 64.0),
                label: label.into(),
                description: description.into(),
                scrolls: true,
            });
            y += 64.0;
        }
        layout.finish_content(y, requested_scroll);
        layout
    }

    pub(crate) fn build_shell(
        width: f32,
        height: f32,
        requested_scroll: f32,
        page: Page,
        query: &str,
        profile_count: usize,
        onboarding_step: Option<OnboardingStep>,
    ) -> Self {
        let mut context = LayoutContext {
            profile_count,
            ..LayoutContext::default()
        };
        if page == Page::Displays {
            context.display_profiles_enabled = true;
        }
        Self::build_shell_with_context(
            width,
            height,
            requested_scroll,
            page,
            query,
            context,
            onboarding_step,
        )
    }

    pub(crate) fn build_shell_with_context(
        width: f32,
        height: f32,
        requested_scroll: f32,
        page: Page,
        query: &str,
        context: LayoutContext,
        onboarding_step: Option<OnboardingStep>,
    ) -> Self {
        let mut layout = Self::shell_base(width, height, page);
        layout.add_chrome();
        if let Some(step) = onboarding_step {
            add_onboarding(&mut layout, step);
        } else if query.trim().is_empty() {
            add_page(&mut layout, page, &context);
        } else {
            add_search_results(&mut layout, query);
        }
        let content_end = layout
            .elements
            .iter()
            .filter(|element| element.scrolls)
            .map(|element| element.rect.bottom())
            .chain(
                layout
                    .regions
                    .iter()
                    .filter(|region| region.scrolls)
                    .map(|region| region.rect.bottom()),
            )
            .chain(
                layout
                    .sections
                    .iter()
                    .map(|section| section.y + section.height),
            )
            .max_by(f32::total_cmp)
            .unwrap_or(layout.content_column.y);
        layout.finish_content(content_end + 32.0, requested_scroll);
        layout
    }

    pub(super) fn shell_base(width: f32, height: f32, page: Page) -> Self {
        let width = width.max(UiTokens::NAV_WIDTH + 360.0);
        let height = height.max(520.0);
        let footer = Rect::new(
            0.0,
            height - UiTokens::FOOTER_HEIGHT,
            width,
            UiTokens::FOOTER_HEIGHT,
        );
        let top_bar = Rect::new(
            UiTokens::NAV_WIDTH,
            0.0,
            width - UiTokens::NAV_WIDTH,
            UiTokens::TOP_BAR_HEIGHT,
        );
        let viewport_top = UiTokens::TOP_BAR_HEIGHT + UiTokens::VIEWPORT_TOP_INSET;
        let content_clip = Rect::new(
            UiTokens::NAV_WIDTH,
            viewport_top,
            (width - UiTokens::NAV_WIDTH).max(320.0),
            (height - viewport_top - UiTokens::FOOTER_HEIGHT).max(180.0),
        );
        let max_width = UiTokens::content_max_width(page);
        let column_width = (content_clip.w - UiTokens::PAGE_MARGIN * 2.0)
            .max(260.0)
            .min(max_width);
        let content_column = Rect::new(
            content_clip.x + UiTokens::PAGE_MARGIN,
            content_clip.y,
            column_width,
            content_clip.h,
        );
        Self {
            width,
            height,
            page,
            content_clip,
            content_column,
            footer,
            elements: Vec::new(),
            regions: Vec::new(),
            sections: Vec::new(),
            max_scroll: 0.0,
            scroll: 0.0,
            nav_width: UiTokens::NAV_WIDTH,
            brand: brand_row_geometry(UiTokens::NAV_WIDTH),
            top_bar,
            search_rect: Rect::new(
                UiTokens::NAV_WIDTH + 32.0,
                (UiTokens::TOP_BAR_HEIGHT - 32.0) * 0.5,
                360.0,
                32.0,
            ),
        }
    }

    pub(super) fn add_chrome(&mut self) {
        let chrome = top_chrome_geometry(self.width, self.nav_width);
        self.top_bar = chrome.row;
        self.search_rect = chrome.search;
        self.elements.push(Element {
            id: ElementId::Search,
            kind: ElementKind::Search,
            rect: self.search_rect,
            label: "Find a setting".into(),
            description: "Search WinShort settings by what you want to do".into(),
            scrolls: false,
        });

        let mut y = UiTokens::NAV_FIRST_ITEM_TOP;
        for page in Page::PRIMARY {
            self.elements.push(Element {
                id: ElementId::Nav(page),
                kind: ElementKind::Navigation,
                rect: Rect::new(16.0, y, self.nav_width - 32.0, 40.0),
                label: page.label().into(),
                description: page.description().into(),
                scrolls: false,
            });
            y += 44.0;
        }
        y += 18.0;
        for page in Page::SECONDARY {
            self.elements.push(Element {
                id: ElementId::Nav(page),
                kind: ElementKind::Navigation,
                rect: Rect::new(16.0, y, self.nav_width - 32.0, 40.0),
                label: page.label().into(),
                description: page.description().into(),
                scrolls: false,
            });
            y += 44.0;
        }

        self.elements.push(Element {
            id: ElementId::WindowClose,
            kind: ElementKind::ButtonSecondary,
            rect: chrome.close,
            label: "Close".into(),
            description: "Hide the Control Center and keep WinShort running".into(),
            scrolls: false,
        });
    }

    pub(super) fn finish_content(&mut self, content_end: f32, requested_scroll: f32) {
        let content_start = self.content_clip.y;
        let max_scroll = (content_end - content_start - self.content_clip.h).max(0.0);
        self.max_scroll = max_scroll;
        self.scroll = requested_scroll.clamp(0.0, max_scroll);
        let dy = -self.scroll;
        for element in &mut self.elements {
            if element.scrolls {
                element.rect = element.rect.translated_y(dy);
            }
        }
        for region in &mut self.regions {
            if region.scrolls {
                region.rect = region.rect.translated_y(dy);
            }
        }
        for section in &mut self.sections {
            section.y += dy;
        }
    }

    pub(crate) fn focus_order(&self) -> Vec<ElementId> {
        self.elements
            .iter()
            .filter(|element| !matches!(element.kind, ElementKind::Card | ElementKind::Info))
            .map(|element| element.id)
            .collect()
    }

    pub(crate) fn hit_test(&self, x: f32, y: f32) -> Option<ElementId> {
        self.elements
            .iter()
            .rev()
            .find(|element| {
                element.rect.contains(x, y)
                    && (!element.scrolls || self.content_clip.contains(x, y))
            })
            .map(|element| element.id)
    }

    pub(crate) fn element(&self, id: ElementId) -> Option<&Element> {
        self.elements.iter().find(|element| element.id == id)
    }
}

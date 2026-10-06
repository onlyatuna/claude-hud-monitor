use crate::layout_engine::{
    distribute_multi_box, find_size, init_empty_multi_box, item_expanding, item_is_empty,
    item_maximum_size, item_minimum_size, item_set_geometry, item_size_hint, q_geom_calc,
    q_max_exp_calc, setup_spacings, LayoutStruct, LAYOUT_SIZE_MAX,
};
use crate::widget::{Widget, WidgetRef};
use qtrs_gui::geometry::primitives::{Margins, Rect, Size};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    TopToBottom,
    LeftToRight,
}

/// `Qt::Alignment` as a layout item uses it (`QLayoutItem::setAlignment`): where the item sits in
/// the space the layout gives it. With no flag on an axis the item fills that axis (up to its
/// maximum size); with a flag it shrinks to its size hint and is placed by the flag.
///
/// The bit values are Qt's (`qnamespace.h:151-170`). This is not the text alignment of a `Label`,
/// which has its own `Alignment`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct ItemAlignment(u32);

impl ItemAlignment {
    pub const NONE: Self = Self(0);
    pub const LEFT: Self = Self(0x1);
    pub const RIGHT: Self = Self(0x2);
    pub const H_CENTER: Self = Self(0x4);
    pub const JUSTIFY: Self = Self(0x8);
    pub const ABSOLUTE: Self = Self(0x10);
    pub const TOP: Self = Self(0x20);
    pub const BOTTOM: Self = Self(0x40);
    pub const V_CENTER: Self = Self(0x80);
    pub const BASELINE: Self = Self(0x100);
    pub const CENTER: Self = Self(0x4 | 0x80);
    /// `Qt::AlignHorizontal_Mask`.
    const HORIZONTAL_MASK: u32 = 0x1 | 0x2 | 0x4 | 0x8 | 0x10;
    /// `Qt::AlignVertical_Mask`.
    const VERTICAL_MASK: u32 = 0x20 | 0x40 | 0x80 | 0x100;

    /// From Qt's numeric flag value; bits that are not alignment flags are dropped.
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits & (Self::HORIZONTAL_MASK | Self::VERTICAL_MASK))
    }

    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Every flag of `other` is set.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Some horizontal flag is set (`align & Qt::AlignHorizontal_Mask`).
    pub const fn horizontal(self) -> bool {
        self.0 & Self::HORIZONTAL_MASK != 0
    }

    /// Some vertical flag is set (`align & Qt::AlignVertical_Mask`).
    pub const fn vertical(self) -> bool {
        self.0 & Self::VERTICAL_MASK != 0
    }
}

impl std::ops::BitOr for ItemAlignment {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

pub struct LayoutItem {
    pub widget: WidgetRef,
    pub stretch: u32,
    /// `QLayoutItem::alignment`.
    pub alignment: ItemAlignment,
    /// A `QSpacerItem`: it has no content of its own, so it takes no spacing.
    pub spacer: bool,
}

pub trait Layout: 'static {
    fn geometry(&self) -> Rect;

    fn set_geometry(&mut self, rect: Rect);

    fn add_widget(&mut self, widget: WidgetRef);

    fn add_widget_with_stretch(&mut self, widget: WidgetRef, stretch: u32);

    /// `QLayout::setAlignment(QWidget*, Qt::Alignment)`: sets the alignment of the item that holds
    /// `widget` (a direct child item only) and re-lays the layout out. Returns false when the
    /// layout has no such item.
    fn set_alignment(&mut self, widget: &WidgetRef, alignment: ItemAlignment) -> bool;

    fn add_stretch(&mut self, stretch: u32) {
        let widget = crate::widget::EmptyWidget::with_geometry(Rect::new(0, 0, 0, 0));
        widget.set_size_policy(crate::size_policy::QSizePolicy::new(
            crate::size_policy::Policy::Expanding,
            crate::size_policy::Policy::Expanding,
        ));
        let spacer = std::rc::Rc::new(std::cell::RefCell::new(
            Box::new(widget) as Box<dyn crate::widget::Widget>,
        ));
        self.add_widget_with_stretch(spacer, stretch.max(1));
    }

    fn widgets(&self) -> Vec<WidgetRef> {
        Vec::new()
    }


    fn set_margins(&mut self, margins: Margins);

    fn margins(&self) -> Margins;

    fn set_spacing(&mut self, spacing: i32);

    fn spacing(&self) -> i32;

    fn size_hint(&self) -> Size;

    /// The smallest size the layout can be squeezed to (`QLayout::minimumSize`).
    fn minimum_size(&self) -> Size;

    /// The directions the layout wants to grow in, as (horizontal, vertical)
    /// (`QLayout::expandingDirections`).
    fn expanding_directions(&self) -> (bool, bool);

    /// Marks this layout as dirty/invalid, requiring recalculation on next activation.
    fn invalidate(&mut self);

    /// Checks if this layout is currently dirty.
    fn is_dirty(&self) -> bool {
        true
    }

    /// Recalculates and positions child items if the layout is dirty.
    fn activate(&mut self);

    /// Convenience and compatibility method: invalidates and activates this layout.
    fn update_layout(&mut self) {
        self.invalidate();
        self.activate();
    }
}

/// Linear box layout arranging items horizontally or vertically (`QBoxLayout`).
pub struct BoxLayout {
    direction: Direction,
    geometry: Rect,
    margins: Margins,
    spacing: i32,
    items: Vec<LayoutItem>,
    dirty: bool,
}

impl BoxLayout {
    pub fn new(direction: Direction) -> Self {
        Self {
            direction,
            geometry: Rect::new(0, 0, 0, 0),
            margins: Margins::new(0, 0, 0, 0),
            spacing: 6,
            items: Vec::new(),
            dirty: true,
        }
    }

    pub fn horizontal() -> Self {
        Self::new(Direction::LeftToRight)
    }

    pub fn vertical() -> Self {
        Self::new(Direction::TopToBottom)
    }

    pub fn direction(&self) -> Direction {
        self.direction
    }

    pub fn set_direction(&mut self, direction: Direction) {
        self.direction = direction;
        self.update_layout();
    }

    pub fn count(&self) -> usize {
        self.items.len()
    }

    pub fn insert_widget(&mut self, index: usize, widget: WidgetRef, stretch: u32) {
        let clamped = index.min(self.items.len());
        self.items.insert(
            clamped,
            LayoutItem { widget, stretch, alignment: ItemAlignment::NONE, spacer: false },
        );
        self.update_layout();
    }

    /// `QBoxLayout::addWidget(widget, stretch, alignment)`.
    pub fn add_widget_aligned(&mut self, widget: WidgetRef, stretch: u32, alignment: ItemAlignment) {
        self.items.push(LayoutItem { widget, stretch, alignment, spacer: false });
        self.update_layout();
    }

    pub fn remove_widget(&mut self, index: usize) -> Option<WidgetRef> {
        if index < self.items.len() {
            let item = self.items.remove(index);
            self.update_layout();
            Some(item.widget)
        } else {
            None
        }
    }
    pub fn add_stretch(&mut self, stretch: u32) {
        <Self as Layout>::add_stretch(self, stretch);
    }

}

impl Layout for BoxLayout {
    fn geometry(&self) -> Rect {
        self.geometry
    }

    fn set_geometry(&mut self, rect: Rect) {
        self.geometry = rect;
        self.dirty = true;
        self.activate();
    }

    fn add_widget(&mut self, widget: WidgetRef) {
        self.add_widget_with_stretch(widget, 0);
    }

    fn add_widget_with_stretch(&mut self, widget: WidgetRef, stretch: u32) {
        self.add_widget_aligned(widget, stretch, ItemAlignment::NONE);
    }

    fn set_alignment(&mut self, widget: &WidgetRef, alignment: ItemAlignment) -> bool {
        let Some(item) = self
            .items
            .iter_mut()
            .find(|item| !item.spacer && std::rc::Rc::ptr_eq(&item.widget, widget))
        else {
            return false;
        };
        item.alignment = alignment;
        self.update_layout();
        true
    }

    /// `QBoxLayout::addStretch`: an empty, expanding spacer that takes no spacing.
    fn add_stretch(&mut self, stretch: u32) {
        let widget = crate::widget::EmptyWidget::with_geometry(Rect::new(0, 0, 0, 0));
        widget.set_size_policy(crate::size_policy::QSizePolicy::new(
            crate::size_policy::Policy::Expanding,
            crate::size_policy::Policy::Expanding,
        ));
        let spacer = std::rc::Rc::new(std::cell::RefCell::new(
            Box::new(widget) as Box<dyn crate::widget::Widget>,
        ));
        self.items.push(LayoutItem {
            widget: spacer,
            stretch: stretch.max(1),
            alignment: ItemAlignment::NONE,
            spacer: true,
        });
        self.update_layout();
    }

    fn widgets(&self) -> Vec<WidgetRef> {
        self.items.iter().map(|it| it.widget.clone()).collect()
    }

    fn set_margins(&mut self, margins: Margins) {
        self.margins = margins;
        self.update_layout();
    }

    fn margins(&self) -> Margins {
        self.margins
    }

    fn set_spacing(&mut self, spacing: i32) {
        self.spacing = spacing;
        self.update_layout();
    }

    fn spacing(&self) -> i32 {
        self.spacing
    }

    fn size_hint(&self) -> Size {
        self.setup_geom().hint
    }

    fn minimum_size(&self) -> Size {
        self.setup_geom().min
    }

    fn expanding_directions(&self) -> (bool, bool) {
        self.setup_geom().expanding
    }

    fn invalidate(&mut self) {
        self.dirty = true;
    }

    fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// `QBoxLayout::setGeometry`.
    fn activate(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;

        if self.items.is_empty() {
            return;
        }

        let horz = self.direction == Direction::LeftToRight;
        let mut chain = self.setup_geom().chain;
        let s = Rect::new(
            self.geometry.x + self.margins.left,
            self.geometry.y + self.margins.top,
            self.geometry.width - self.margins.left - self.margins.right,
            self.geometry.height - self.margins.top - self.margins.bottom,
        );
        let (pos, space) = if horz { (s.x, s.width) } else { (s.y, s.height) };
        let n = chain.len();
        q_geom_calc(&mut chain, 0, n, pos, space, -1);

        for (item, data) in self.items.iter().zip(&chain) {
            let rect = if horz {
                Rect::new(data.pos, s.y, data.size, s.height)
            } else {
                Rect::new(s.x, data.pos, s.width, data.size)
            };
            let old_size = {
                let w = item.widget.borrow();
                let g = w.geometry();
                Size::new(g.width, g.height)
            };
            if item.spacer {
                // A `QSpacerItem` just remembers its rectangle.
                item.widget.borrow().set_geometry(rect);
            } else {
                item_set_geometry(&**item.widget.borrow(), rect, item.alignment);
            }
            let new_size = Size::new(rect.width, rect.height);
            if let Some(child_layout) = item.widget.borrow().layout_ref_mut() {
                if old_size != new_size || child_layout.is_dirty() {
                    crate::layout_scheduler::LayoutScheduler::invalidate(&item.widget);
                }
            }
        }
    }
}

/// What `QBoxLayoutPrivate::setupGeom` computes.
struct BoxGeom {
    chain: Vec<LayoutStruct>,
    min: Size,
    hint: Size,
    expanding: (bool, bool),
}

impl BoxLayout {
    /// `QBoxLayoutPrivate::setupGeom`: the chain handed to `qGeomCalc` and the layout's sizes.
    fn setup_geom(&self) -> BoxGeom {
        let horz = self.direction == Direction::LeftToRight;
        // Along the layout the maximum sizes add up; across it they are folded by `qMaxExpCalc`.
        let mut max_main: i64 = 0;
        let mut max_cross: i32 = LAYOUT_SIZE_MAX;
        let (mut min_main, mut hint_main) = (0i32, 0i32);
        let (mut min_cross, mut hint_cross) = (0i32, 0i32);
        let (mut main_exp, mut cross_exp) = (false, false);

        let mut chain = vec![LayoutStruct::default(); self.items.len()];
        let mut previous_non_empty: Option<usize> = None;

        for (i, item) in self.items.iter().enumerate() {
            // Everything below is (main axis, cross axis).
            let (max, min, hint, exp, empty, is_widget, policy_stretch) = if item.spacer {
                // `QSpacerItem(0, 0, Expanding, Minimum)`, transposed for a vertical layout.
                ((LAYOUT_SIZE_MAX, LAYOUT_SIZE_MAX), (0, 0), (0, 0), (true, false), true, false, 0)
            } else {
                let w = item.widget.borrow();
                let policy = w.size_policy();
                let swap = |s: Size| if horz { (s.width, s.height) } else { (s.height, s.width) };
                let exp = item_expanding(&**w, item.alignment);
                (
                    swap(item_maximum_size(&**w, item.alignment)),
                    swap(item_minimum_size(&**w)),
                    swap(item_size_hint(&**w)),
                    if horz { exp } else { (exp.1, exp.0) },
                    item_is_empty(&**w),
                    true,
                    (if horz { policy.horizontal_stretch } else { policy.vertical_stretch }) as i32,
                )
            };

            let mut spacing = 0;
            if !empty {
                spacing = if previous_non_empty.is_some() { self.spacing } else { 0 };
                if let Some(previous) = previous_non_empty {
                    chain[previous].spacing = spacing;
                }
                previous_non_empty = Some(i);
            }

            let expand = exp.0 || item.stretch > 0;
            main_exp = main_exp || expand;
            max_main += (spacing + max.0) as i64;
            min_main += spacing + min.0;
            hint_main += spacing + hint.0;
            if !(empty && is_widget) {
                // hidden widgets are ignored
                let mut dummy = true;
                q_max_exp_calc(&mut max_cross, &mut cross_exp, &mut dummy, max.1, exp.1, empty);
            }
            min_cross = min_cross.max(min.1);
            hint_cross = hint_cross.max(hint.1);

            chain[i].size_hint = hint.0;
            chain[i].maximum_size = max.0;
            chain[i].minimum_size = min.0;
            chain[i].expansive = expand;
            chain[i].stretch = if item.stretch > 0 { item.stretch as i32 } else { policy_stretch };
            chain[i].empty = empty;
            chain[i].spacing = 0; // may be set non-zero by a later non-empty item
        }

        let extra = Size::new(self.margins.left + self.margins.right, self.margins.top + self.margins.bottom);
        let (min_w, min_h) = if horz { (min_main, min_cross) } else { (min_cross, min_main) };
        let (hint_w, hint_h) = if horz { (hint_main, hint_cross) } else { (hint_cross, hint_main) };
        let (max_w, max_h) = if horz {
            (max_main, max_cross as i64)
        } else {
            (max_cross as i64, max_main)
        };
        // `maxSize = QSize(maxw, maxh).expandedTo(minSize)`; `sizeHint` is bounded by both.
        let bounded = |hint: i32, min: i32, max: i64| hint.max(min).min(max.max(min as i64).min(i32::MAX as i64) as i32);
        BoxGeom {
            chain,
            min: Size::new(min_w + extra.width, min_h + extra.height),
            hint: Size::new(
                bounded(hint_w, min_w, max_w) + extra.width,
                bounded(hint_h, min_h, max_h) + extra.height,
            ),
            expanding: if horz { (main_exp, cross_exp) } else { (cross_exp, main_exp) },
        }
    }
}

/// Item inside a GridLayout spanning a grid area (`QGridLayout`).
pub struct GridItem {
    pub widget: WidgetRef,
    pub row: usize,
    pub column: usize,
    pub row_span: usize,
    pub col_span: usize,
    /// `QLayoutItem::alignment`.
    pub alignment: ItemAlignment,
}

/// Grid layout laying out widgets in a 2D grid of rows and columns (`QGridLayout`).
pub struct GridLayout {
    geometry: Rect,
    margins: Margins,
    h_spacing: i32,
    v_spacing: i32,
    items: Vec<GridItem>,
    row_stretches: Vec<u32>,
    col_stretches: Vec<u32>,
    col_min_widths: Vec<i32>,
    row_min_heights: Vec<i32>,
    dirty: bool,
}

impl GridLayout {
    pub fn new() -> Self {
        Self {
            geometry: Rect::new(0, 0, 0, 0),
            margins: Margins::new(0, 0, 0, 0),
            h_spacing: 6,
            v_spacing: 6,
            items: Vec::new(),
            row_stretches: Vec::new(),
            col_stretches: Vec::new(),
            col_min_widths: Vec::new(),
            row_min_heights: Vec::new(),
            dirty: true,
        }
    }

    pub fn add_widget(&mut self, widget: WidgetRef, row: usize, column: usize) {
        self.add_widget_with_span(widget, row, column, 1, 1);
    }

    pub fn add_widget_with_span(
        &mut self,
        widget: WidgetRef,
        row: usize,
        column: usize,
        row_span: usize,
        col_span: usize,
    ) {
        self.add_widget_aligned(widget, row, column, row_span, col_span, ItemAlignment::NONE);
    }

    /// `QGridLayout::addWidget(widget, row, column, rowSpan, columnSpan, alignment)`.
    pub fn add_widget_aligned(
        &mut self,
        widget: WidgetRef,
        row: usize,
        column: usize,
        row_span: usize,
        col_span: usize,
        alignment: ItemAlignment,
    ) {
        self.items.push(GridItem {
            widget,
            row,
            column,
            row_span: row_span.max(1),
            col_span: col_span.max(1),
            alignment,
        });
        self.update_layout();
    }

    pub fn set_row_stretch(&mut self, row: usize, stretch: u32) {
        if row >= self.row_stretches.len() {
            self.row_stretches.resize(row + 1, 0);
        }
        self.row_stretches[row] = stretch;
        self.update_layout();
    }

    pub fn set_column_stretch(&mut self, col: usize, stretch: u32) {
        if col >= self.col_stretches.len() {
            self.col_stretches.resize(col + 1, 0);
        }
        self.col_stretches[col] = stretch;
        self.update_layout();
    }

    /// `QGridLayout::setColumnMinimumWidth` equivalent: the column is laid out at least this
    /// wide even when no item in it asks for it.
    pub fn set_column_minimum_width(&mut self, col: usize, min_w: i32) {
        if col >= self.col_min_widths.len() {
            self.col_min_widths.resize(col + 1, 0);
        }
        self.col_min_widths[col] = min_w;
        self.update_layout();
    }

    /// `QGridLayout::setRowMinimumHeight` equivalent: the row is laid out at least this tall
    /// even when no item in it asks for it.

    pub fn set_row_minimum_height(&mut self, row: usize, min_h: i32) {
        if row >= self.row_min_heights.len() {
            self.row_min_heights.resize(row + 1, 0);
        }
        self.row_min_heights[row] = min_h;
        self.update_layout();
    }

    pub fn set_horizontal_spacing(&mut self, spacing: i32) {
        self.h_spacing = spacing;
        self.update_layout();
    }

    pub fn set_vertical_spacing(&mut self, spacing: i32) {
        self.v_spacing = spacing;
        self.update_layout();
    }

    /// `QGridLayoutPrivate::expand`: rows and columns grow with the items and with any stretch
    /// or minimum size set for them.
    pub fn row_count(&self) -> usize {
        self.items
            .iter()
            .map(|it| it.row + it.row_span)
            .chain([self.row_stretches.len(), self.row_min_heights.len()])
            .max()
            .unwrap_or(0)
    }

    pub fn column_count(&self) -> usize {
        self.items
            .iter()
            .map(|it| it.column + it.col_span)
            .chain([self.col_stretches.len(), self.col_min_widths.len()])
            .max()
            .unwrap_or(0)
    }
}

impl Default for GridLayout {
    fn default() -> Self {
        Self::new()
    }
}

impl Layout for GridLayout {
    fn geometry(&self) -> Rect {
        self.geometry
    }

    fn set_geometry(&mut self, rect: Rect) {
        self.geometry = rect;
        self.dirty = true;
        self.activate();
    }

    fn add_widget(&mut self, widget: WidgetRef) {
        let r = self.row_count();
        self.add_widget(widget, r, 0);
    }

    fn add_widget_with_stretch(&mut self, widget: WidgetRef, _stretch: u32) {
        let r = self.row_count();
        self.add_widget(widget, r, 0);
    }

    fn set_alignment(&mut self, widget: &WidgetRef, alignment: ItemAlignment) -> bool {
        let Some(item) = self
            .items
            .iter_mut()
            .find(|item| std::rc::Rc::ptr_eq(&item.widget, widget))
        else {
            return false;
        };
        item.alignment = alignment;
        self.update_layout();
        true
    }


    fn widgets(&self) -> Vec<WidgetRef> {
        self.items.iter().map(|it| it.widget.clone()).collect()
    }
    fn set_margins(&mut self, margins: Margins) {
        self.margins = margins;
        self.update_layout();
    }

    fn margins(&self) -> Margins {
        self.margins
    }

    fn set_spacing(&mut self, spacing: i32) {
        self.h_spacing = spacing;
        self.v_spacing = spacing;
        self.update_layout();
    }

    fn spacing(&self) -> i32 {
        self.h_spacing
    }

    fn size_hint(&self) -> Size {
        let (rows, cols) = self.setup_layout_data();
        let size = find_size(&rows, &cols, |d| d.size_hint);
        Size::new(
            size.width + self.margins.left + self.margins.right,
            size.height + self.margins.top + self.margins.bottom,
        )
    }

    fn minimum_size(&self) -> Size {
        let (rows, cols) = self.setup_layout_data();
        let size = find_size(&rows, &cols, |d| d.minimum_size);
        Size::new(
            size.width + self.margins.left + self.margins.right,
            size.height + self.margins.top + self.margins.bottom,
        )
    }

    fn expanding_directions(&self) -> (bool, bool) {
        let (rows, cols) = self.setup_layout_data();
        (cols.iter().any(|c| c.expansive), rows.iter().any(|r| r.expansive))
    }

    fn invalidate(&mut self) {
        self.dirty = true;
    }

    fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// `QGridLayoutPrivate::distribute`.
    fn activate(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;

        let (mut rows, mut cols) = self.setup_layout_data();
        let (rr, cc) = (rows.len(), cols.len());
        if rr == 0 || cc == 0 {
            return;
        }

        let x = self.geometry.x + self.margins.left;
        let y = self.geometry.y + self.margins.top;
        let width = self.geometry.width - self.margins.left - self.margins.right;
        let height = self.geometry.height - self.margins.top - self.margins.bottom;
        q_geom_calc(&mut cols, 0, cc, x, width, -1);
        q_geom_calc(&mut rows, 0, rr, y, height, -1);

        for item in &self.items {
            let (r2, c2) = (item.row + item.row_span - 1, item.column + item.col_span - 1);
            let left = cols[item.column].pos;
            let top = rows[item.row].pos;
            let w = cols[c2].pos + cols[c2].size - left;
            let h = rows[r2].pos + rows[r2].size - top;

            let old_size = {
                let widget = item.widget.borrow();
                let g = widget.geometry();
                Size::new(g.width, g.height)
            };
            item_set_geometry(&**item.widget.borrow(), Rect::new(left, top, w, h), item.alignment);
            let new_size = Size::new(w, h);
            if let Some(child_layout) = item.widget.borrow().layout_ref_mut() {
                if old_size != new_size || child_layout.is_dirty() {
                    crate::layout_scheduler::LayoutScheduler::invalidate(&item.widget);
                }
            }
        }
    }
}

impl GridLayout {
    /// `QGridLayoutPrivate::setupLayoutData`: the row and column chains of the current items.
    fn setup_layout_data(&self) -> (Vec<LayoutStruct>, Vec<LayoutStruct>) {
        let rr = self.row_count();
        let cc = self.column_count();
        let stretch_of = |v: &[u32], i: usize| v.get(i).copied().unwrap_or(0) as i32;
        let min_of = |v: &[i32], i: usize| v.get(i).copied().unwrap_or(0);

        let mut rows = vec![LayoutStruct::default(); rr];
        let mut cols = vec![LayoutStruct::default(); cc];
        for (i, row) in rows.iter_mut().enumerate() {
            let (stretch, min) = (stretch_of(&self.row_stretches, i), min_of(&self.row_min_heights, i));
            row.init(stretch, min);
            row.maximum_size = if stretch != 0 { LAYOUT_SIZE_MAX } else { min };
        }
        for (i, col) in cols.iter_mut().enumerate() {
            let (stretch, min) = (stretch_of(&self.col_stretches, i), min_of(&self.col_min_widths, i));
            col.init(stretch, min);
            col.maximum_size = if stretch != 0 { LAYOUT_SIZE_MAX } else { min };
        }

        struct Boxed {
            min: Size,
            hint: Size,
            max: Size,
            expanding: (bool, bool),
            empty: bool,
            h_stretch: i32,
            v_stretch: i32,
        }
        let boxes: Vec<Boxed> = self
            .items
            .iter()
            .map(|item| {
                let w = item.widget.borrow();
                let policy = w.size_policy();
                Boxed {
                    min: item_minimum_size(&**w),
                    hint: item_size_hint(&**w),
                    max: item_maximum_size(&**w, item.alignment),
                    expanding: item_expanding(&**w, item.alignment),
                    empty: item_is_empty(&**w),
                    h_stretch: policy.horizontal_stretch as i32,
                    v_stretch: policy.vertical_stretch as i32,
                }
            })
            .collect();

        // Which item covers which cell, to find the neighbours the spacing goes between.
        let mut grid: Vec<Option<usize>> = vec![None; rr * cc];
        let mut has_multi = false;
        for (i, item) in self.items.iter().enumerate() {
            let b = &boxes[i];
            let (to_row, to_col) = (item.row + item.row_span - 1, item.column + item.col_span - 1);

            if item.row == to_row {
                if !b.empty {
                    let data = &mut rows[item.row];
                    if stretch_of(&self.row_stretches, item.row) == 0 {
                        data.stretch = data.stretch.max(b.v_stretch);
                    }
                    data.size_hint = data.size_hint.max(b.hint.height);
                    data.minimum_size = data.minimum_size.max(b.min.height);
                    q_max_exp_calc(&mut data.maximum_size, &mut data.expansive, &mut data.empty, b.max.height, b.expanding.1, b.empty);
                }
            } else {
                init_empty_multi_box(&mut rows, item.row, to_row);
                has_multi = true;
            }

            if item.column == to_col {
                if !b.empty {
                    let data = &mut cols[item.column];
                    if stretch_of(&self.col_stretches, item.column) == 0 {
                        data.stretch = data.stretch.max(b.h_stretch);
                    }
                    data.size_hint = data.size_hint.max(b.hint.width);
                    data.minimum_size = data.minimum_size.max(b.min.width);
                    q_max_exp_calc(&mut data.maximum_size, &mut data.expansive, &mut data.empty, b.max.width, b.expanding.0, b.empty);
                }
            } else {
                init_empty_multi_box(&mut cols, item.column, to_col);
                has_multi = true;
            }

            for r in item.row..=to_row {
                for c in item.column..=to_col {
                    grid[r * cc + c] = Some(i);
                }
            }
        }

        let empty_of = |i: usize| boxes[i].empty;
        setup_spacings(&mut cols, &grid, cc, self.h_spacing, true, &empty_of);
        setup_spacings(&mut rows, &grid, cc, self.v_spacing, false, &empty_of);

        // Multi-cell items go in after the single-cell ones for a better distribution.
        if has_multi {
            for (i, item) in self.items.iter().enumerate() {
                let b = &boxes[i];
                let (to_row, to_col) = (item.row + item.row_span - 1, item.column + item.col_span - 1);
                if item.row != to_row {
                    distribute_multi_box(&mut rows, item.row, to_row, b.min.height, b.hint.height, &self.row_stretches, b.v_stretch);
                }
                if item.column != to_col {
                    distribute_multi_box(&mut cols, item.column, to_col, b.min.width, b.hint.width, &self.col_stretches, b.h_stretch);
                }
            }
        }

        for row in &mut rows {
            row.expansive = row.expansive || row.stretch > 0;
        }
        for col in &mut cols {
            col.expansive = col.expansive || col.stretch > 0;
        }
        (rows, cols)
    }
}

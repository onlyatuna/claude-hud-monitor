use crate::size_policy::Policy;
use crate::widget::{Widget, WidgetRef};
use qtrs_gui::geometry::primitives::{Margins, Rect, Size};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    TopToBottom,
    LeftToRight,
}

pub struct LayoutItem {
    pub widget: WidgetRef,
    pub stretch: u32,
}

pub trait Layout: 'static {
    fn geometry(&self) -> Rect;

    fn set_geometry(&mut self, rect: Rect);

    fn add_widget(&mut self, widget: WidgetRef);

    fn add_widget_with_stretch(&mut self, widget: WidgetRef, stretch: u32);
    fn add_stretch(&mut self, stretch: u32) {
        let mut widget = crate::widget::EmptyWidget::with_geometry(Rect::new(0, 0, 0, 0));
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

/// Distributes 1D space among items based on hints, min/max constraints, policies, and stretch.
pub fn distribute_1d_space(
    items: &[(i32, i32, i32, Policy, u32)], // (hint, min, max, policy, stretch)
    available_span: i32,
) -> Vec<i32> {
    let count = items.len();
    if count == 0 {
        return Vec::new();
    }

    let explicit_stretch_sum: u32 = items.iter().map(|it| it.4).sum();

    if explicit_stretch_sum > 0 {
        let mut sizes = vec![0i32; count];
        let mut non_stretch_sum = 0i32;

        for (i, &(hint, min_sz, max_sz, policy, stretch)) in items.iter().enumerate() {
            if stretch == 0 {
                let base = match policy {
                    Policy::Ignored => min_sz,
                    Policy::Fixed => hint,
                    Policy::Minimum | Policy::MinimumExpanding => hint.max(min_sz),
                    Policy::Maximum => hint.min(max_sz),
                    Policy::Preferred | Policy::Expanding => hint,
                };
                let s = base.clamp(min_sz, max_sz);
                sizes[i] = s;
                non_stretch_sum += s;
            }
        }

        let stretch_space = (available_span - non_stretch_sum).max(0);
        let mut allocated_stretch = 0i32;

        for (i, &(_, min_sz, max_sz, _, stretch)) in items.iter().enumerate() {
            if stretch > 0 {
                let s =
                    ((stretch_space as i64 * stretch as i64) / explicit_stretch_sum as i64) as i32;
                let clamped = s.clamp(min_sz, max_sz);
                sizes[i] = clamped;
                allocated_stretch += clamped;
            }
        }

        let mut rem_slack = stretch_space - allocated_stretch;
        if rem_slack > 0 {
            for (i, &(_, _, max_sz, _, stretch)) in items.iter().enumerate() {
                if stretch > 0 && sizes[i] < max_sz && rem_slack > 0 {
                    sizes[i] += 1;
                    rem_slack -= 1;
                }
            }
        }

        return sizes;
    }

    // 1. Initial base size calculation
    let mut sizes: Vec<i32> = items
        .iter()
        .map(|&(hint, min_sz, max_sz, policy, _)| {
            let base = match policy {
                Policy::Ignored => min_sz,
                Policy::Fixed => hint,
                Policy::Minimum | Policy::MinimumExpanding => hint.max(min_sz),
                Policy::Maximum => hint.min(max_sz),
                Policy::Preferred | Policy::Expanding => hint,
            };
            base.clamp(min_sz, max_sz)
        })
        .collect();

    let current_sum: i32 = sizes.iter().sum();
    let mut slack = available_span - current_sum;

    // 2. Expand if slack > 0
    if slack > 0 {
        for _ in 0..10 {
            if slack <= 0 {
                break;
            }

            let mut eligible: Vec<usize> = Vec::new();
            let mut total_stretch: u32 = 0;

            for (i, &(_, _, max_sz, policy, stretch)) in items.iter().enumerate() {
                if sizes[i] < max_sz && (policy.can_grow() || stretch > 0) {
                    eligible.push(i);
                    let effective_stretch = if stretch > 0 {
                        stretch
                    } else if policy.is_expanding() {
                        1
                    } else {
                        0
                    };
                    total_stretch += effective_stretch;
                }
            }

            if eligible.is_empty() {
                break;
            }

            let remaining_slack = slack;
            let mut allocated_this_round = 0;

            for &i in &eligible {
                let (_, _, max_sz, _, stretch) = items[i];
                let effective_stretch = if stretch > 0 {
                    stretch
                } else if items[i].3.is_expanding() {
                    1
                } else {
                    0
                };

                let share = if total_stretch > 0 {
                    if effective_stretch > 0 {
                        ((remaining_slack as i64 * effective_stretch as i64) / total_stretch as i64)
                            as i32
                    } else {
                        0
                    }
                } else {
                    remaining_slack / eligible.len() as i32
                };

                let add = share
                    .min(max_sz - sizes[i])
                    .min(slack - allocated_this_round);
                if add > 0 {
                    sizes[i] += add;
                    allocated_this_round += add;
                }
            }

            slack -= allocated_this_round;
            if allocated_this_round == 0 {
                // Integer division remainder fallback: 1px allocation to first eligible
                for &i in &eligible {
                    let (_, _, max_sz, _, _) = items[i];
                    if sizes[i] < max_sz && slack > 0 {
                        sizes[i] += 1;
                        slack -= 1;
                    }
                }
                break;
            }
        }
    } else if slack < 0 {
        // 3. Shrink if slack < 0
        let mut deficit = -slack;
        for _ in 0..10 {
            if deficit <= 0 {
                break;
            }

            let eligible: Vec<usize> = items
                .iter()
                .enumerate()
                .filter_map(|(i, &(_, min_sz, _, policy, _))| {
                    if sizes[i] > min_sz && policy.can_shrink() {
                        Some(i)
                    } else {
                        None
                    }
                })
                .collect();

            if eligible.is_empty() {
                break;
            }

            let per_item = (deficit / eligible.len() as i32).max(1);
            let mut reduced_this_round = 0;

            for &i in &eligible {
                let (_, min_sz, _, _, _) = items[i];
                let can_reduce = (sizes[i] - min_sz)
                    .min(deficit - reduced_this_round)
                    .min(per_item);
                if can_reduce > 0 {
                    sizes[i] -= can_reduce;
                    reduced_this_round += can_reduce;
                }
            }

            deficit -= reduced_this_round;
            if reduced_this_round == 0 {
                break;
            }
        }
    }

    sizes
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
        self.items.insert(clamped, LayoutItem { widget, stretch });
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
        self.items.push(LayoutItem { widget, stretch });
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
        if self.items.is_empty() {
            return Size::new(
                self.margins.left + self.margins.right,
                self.margins.top + self.margins.bottom,
            );
        }

        let mut total_w = 0;
        let mut total_h = 0;
        let mut visible_count = 0;

        for item in &self.items {
            if !item.widget.borrow().is_visible() {
                continue;
            }
            visible_count += 1;
            if item.stretch > 0 {
                continue;
            }
            let hint = item.widget.borrow().size_hint();
            match self.direction {
                Direction::TopToBottom => {
                    total_w = total_w.max(hint.width);
                    total_h += hint.height;
                }
                Direction::LeftToRight => {
                    total_w += hint.width;
                    total_h = total_h.max(hint.height);
                }
            }
        }

        let total_spacing = (visible_count - 1).max(0) * self.spacing;
        match self.direction {
            Direction::TopToBottom => total_h += total_spacing,
            Direction::LeftToRight => total_w += total_spacing,
        }

        Size::new(
            total_w + self.margins.left + self.margins.right,
            total_h + self.margins.top + self.margins.bottom,
        )
    }

    fn invalidate(&mut self) {
        self.dirty = true;
    }

    fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn activate(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;

        if self.items.is_empty() {
            return;
        }

        // Filter visible items and ensure hidden items are given zero geometry
        let visible_items: Vec<(usize, &LayoutItem)> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, it)| {
                let vis = it.widget.borrow().is_visible();
                if !vis {
                    it.widget.borrow().set_geometry(Rect::new(0, 0, 0, 0));
                }
                vis
            })
            .collect();

        if visible_items.is_empty() {
            return;
        }

        let avail_x = self.geometry.x + self.margins.left;
        let avail_y = self.geometry.y + self.margins.top;
        let avail_w = (self.geometry.width - self.margins.left - self.margins.right).max(0);
        let avail_h = (self.geometry.height - self.margins.top - self.margins.bottom).max(0);

        let count = visible_items.len();
        let total_spacing = ((count - 1) as i32).max(0) * self.spacing;

        match self.direction {
            Direction::TopToBottom => {
                let net_height = (avail_h - total_spacing).max(0);
                let item_specs: Vec<(i32, i32, i32, Policy, u32)> = visible_items
                    .iter()
                    .map(|(_, it)| {
                        let w = it.widget.borrow();
                        let hint = w.size_hint().height;
                        let min_sz = w.minimum_size().height;
                        let max_sz = w.maximum_size().height;
                        let policy = w.size_policy().vertical;
                        let stretch = it.stretch.max(w.size_policy().vertical_stretch);
                        (hint, min_sz, max_sz, policy, stretch)
                    })
                    .collect();

                let heights = distribute_1d_space(&item_specs, net_height);

                let mut cur_y = avail_y;
                for (v_idx, (_, item)) in visible_items.iter().enumerate() {
                    let item_h = heights[v_idx];
                    let w = item.widget.borrow();
                    let cross_policy = w.size_policy().horizontal;
                    let cross_hint = w.size_hint().width;
                    let cross_min = w.minimum_size().width;
                    let cross_max = w.maximum_size().width;
                    drop(w);

                    let item_w = if cross_policy == Policy::Fixed {
                        cross_hint.clamp(cross_min, cross_max)
                    } else {
                        avail_w.clamp(cross_min, cross_max)
                    };

                    let item_rect = Rect::new(avail_x, cur_y, item_w, item_h);
                    let old_size = {
                        let w = item.widget.borrow();
                        let g = w.geometry();
                        Size::new(g.width, g.height)
                    };
                    item.widget.borrow().set_geometry(item_rect);
                    let new_size = Size::new(item_w, item_h);
                    if let Some(child_layout) = item.widget.borrow().layout_ref_mut() {
                        if old_size != new_size || child_layout.is_dirty() {
                            crate::layout_scheduler::LayoutScheduler::invalidate(&item.widget);
                        }
                    }
                    cur_y += item_h + self.spacing;
                }
            }
            Direction::LeftToRight => {
                let net_width = (avail_w - total_spacing).max(0);
                let item_specs: Vec<(i32, i32, i32, Policy, u32)> = visible_items
                    .iter()
                    .map(|(_, it)| {
                        let w = it.widget.borrow();
                        let hint = w.size_hint().width;
                        let min_sz = w.minimum_size().width;
                        let max_sz = w.maximum_size().width;
                        let policy = w.size_policy().horizontal;
                        let stretch = it.stretch.max(w.size_policy().horizontal_stretch);
                        (hint, min_sz, max_sz, policy, stretch)
                    })
                    .collect();

                let widths = distribute_1d_space(&item_specs, net_width);

                let mut cur_x = avail_x;
                for (v_idx, (_, item)) in visible_items.iter().enumerate() {
                    let item_w = widths[v_idx];
                    let w = item.widget.borrow();
                    let cross_policy = w.size_policy().vertical;
                    let cross_hint = w.size_hint().height;
                    let cross_min = w.minimum_size().height;
                    let cross_max = w.maximum_size().height;
                    drop(w);

                    let item_h = if cross_policy == Policy::Fixed {
                        cross_hint.clamp(cross_min, cross_max)
                    } else {
                        avail_h.clamp(cross_min, cross_max)
                    };
                    let offset_y = if item_h < avail_h {
                        (avail_h - item_h) / 2
                    } else {
                        0
                    };

                    let item_rect = Rect::new(cur_x, avail_y + offset_y, item_w, item_h);
                    let old_size = {
                        let w = item.widget.borrow();
                        let g = w.geometry();
                        Size::new(g.width, g.height)
                    };
                    item.widget.borrow().set_geometry(item_rect);
                    let new_size = Size::new(item_w, item_h);
                    if let Some(child_layout) = item.widget.borrow().layout_ref_mut() {
                        if old_size != new_size || child_layout.is_dirty() {
                            crate::layout_scheduler::LayoutScheduler::invalidate(&item.widget);
                        }
                    }
                    cur_x += item_w + self.spacing;
                }
            }
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
        self.items.push(GridItem {
            widget,
            row,
            column,
            row_span: row_span.max(1),
            col_span: col_span.max(1),
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

    pub fn set_horizontal_spacing(&mut self, spacing: i32) {
        self.h_spacing = spacing;
        self.update_layout();
    }

    pub fn set_vertical_spacing(&mut self, spacing: i32) {
        self.v_spacing = spacing;
        self.update_layout();
    }

    pub fn row_count(&self) -> usize {
        self.items
            .iter()
            .map(|it| it.row + it.row_span)
            .max()
            .unwrap_or(0)
    }

    pub fn column_count(&self) -> usize {
        self.items
            .iter()
            .map(|it| it.column + it.col_span)
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
        let rows = self.row_count();
        let cols = self.column_count();
        if rows == 0 || cols == 0 {
            return Size::new(
                self.margins.left + self.margins.right,
                self.margins.top + self.margins.bottom,
            );
        }

        let mut col_widths = vec![0i32; cols];
        let mut row_heights = vec![0i32; rows];

        for item in &self.items {
            let hint = item.widget.borrow().size_hint();
            if item.col_span == 1 {
                col_widths[item.column] = col_widths[item.column].max(hint.width);
            }
            if item.row_span == 1 {
                row_heights[item.row] = row_heights[item.row].max(hint.height);
            }
        }

        let total_w: i32 = col_widths.iter().sum::<i32>()
            + ((cols - 1) as i32).max(0) * self.h_spacing
            + self.margins.left
            + self.margins.right;
        let total_h: i32 = row_heights.iter().sum::<i32>()
            + ((rows - 1) as i32).max(0) * self.v_spacing
            + self.margins.top
            + self.margins.bottom;

        Size::new(total_w, total_h)
    }

    fn invalidate(&mut self) {
        self.dirty = true;
    }

    fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn activate(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;

        let rows = self.row_count();
        let cols = self.column_count();
        if rows == 0 || cols == 0 {
            return;
        }

        let avail_x = self.geometry.x + self.margins.left;
        let avail_y = self.geometry.y + self.margins.top;
        let avail_w = (self.geometry.width - self.margins.left - self.margins.right).max(0);
        let avail_h = (self.geometry.height - self.margins.top - self.margins.bottom).max(0);

        let total_h_spacing = ((cols - 1) as i32).max(0) * self.h_spacing;
        let total_v_spacing = ((rows - 1) as i32).max(0) * self.v_spacing;

        let net_w = (avail_w - total_h_spacing).max(0);
        let net_h = (avail_h - total_v_spacing).max(0);

        // Gather col specs
        let mut col_specs = Vec::with_capacity(cols);
        for c in 0..cols {
            let mut hint = 0;
            let mut min_sz = 0;
            let mut max_sz = 16777215;
            let mut policy = Policy::Preferred;
            for item in &self.items {
                if item.column == c && item.col_span == 1 {
                    let w = item.widget.borrow();
                    hint = hint.max(w.size_hint().width);
                    min_sz = min_sz.max(w.minimum_size().width);
                    max_sz = max_sz.min(w.maximum_size().width);
                    if w.size_policy().horizontal.is_expanding() {
                        policy = Policy::Expanding;
                    }
                }
            }
            let stretch = self.col_stretches.get(c).copied().unwrap_or(0);
            col_specs.push((hint, min_sz, max_sz, policy, stretch));
        }
        let col_widths = distribute_1d_space(&col_specs, net_w);

        // Gather row specs
        let mut row_specs = Vec::with_capacity(rows);
        for r in 0..rows {
            let mut hint = 0;
            let mut min_sz = 0;
            let mut max_sz = 16777215;
            let mut policy = Policy::Preferred;
            for item in &self.items {
                if item.row == r && item.row_span == 1 {
                    let w = item.widget.borrow();
                    hint = hint.max(w.size_hint().height);
                    min_sz = min_sz.max(w.minimum_size().height);
                    max_sz = max_sz.min(w.maximum_size().height);
                    if w.size_policy().vertical.is_expanding() {
                        policy = Policy::Expanding;
                    }
                }
            }
            let stretch = self.row_stretches.get(r).copied().unwrap_or(0);
            row_specs.push((hint, min_sz, max_sz, policy, stretch));
        }
        let row_heights = distribute_1d_space(&row_specs, net_h);

        // Compute row Y offsets and col X offsets
        let mut col_x = Vec::with_capacity(cols);
        let mut cur_x = avail_x;
        for &w in &col_widths {
            col_x.push(cur_x);
            cur_x += w + self.h_spacing;
        }

        let mut row_y = Vec::with_capacity(rows);
        let mut cur_y = avail_y;
        for &h in &row_heights {
            row_y.push(cur_y);
            cur_y += h + self.v_spacing;
        }

        // Position items
        for item in &self.items {
            let x = col_x[item.column];
            let y = row_y[item.row];

            let w: i32 = col_widths[item.column..(item.column + item.col_span).min(cols)]
                .iter()
                .sum::<i32>()
                + ((item.col_span - 1) as i32).max(0) * self.h_spacing;

            let h: i32 = row_heights[item.row..(item.row + item.row_span).min(rows)]
                .iter()
                .sum::<i32>()
                + ((item.row_span - 1) as i32).max(0) * self.v_spacing;
            let old_size = {
                let w = item.widget.borrow();
                let g = w.geometry();
                Size::new(g.width, g.height)
            };
            item.widget.borrow().set_geometry(Rect::new(x, y, w, h));
            let new_size = Size::new(w, h);
            if let Some(child_layout) = item.widget.borrow().layout_ref_mut() {
                if old_size != new_size || child_layout.is_dirty() {
                    crate::layout_scheduler::LayoutScheduler::invalidate(&item.widget);
                }
            }
        }
    }
}

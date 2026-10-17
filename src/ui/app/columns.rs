use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Column {
    Name,
    Author,
    Size,
    Modified,
    Status,
}

impl Column {
    const ALL: [Self; 5] = [
        Self::Name,
        Self::Author,
        Self::Size,
        Self::Modified,
        Self::Status,
    ];

    fn limits(self) -> (f32, f32) {
        match self {
            Self::Name => (180., f32::MAX),
            Self::Author => (72., 360.),
            Self::Size => (64., 200.),
            Self::Modified => (136., 280.),
            Self::Status => (100., 220.),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct ColumnWidths {
    author: f32,
    size: f32,
    modified: f32,
    status: f32,
}

impl Default for ColumnWidths {
    fn default() -> Self {
        Self {
            author: 88.,
            size: 76.,
            modified: 136.,
            status: 100.,
        }
    }
}

impl ColumnWidths {
    fn get(self, column: Column) -> f32 {
        let value = match column {
            Column::Name => return 180.,
            Column::Author => self.author,
            Column::Size => self.size,
            Column::Modified => self.modified,
            Column::Status => self.status,
        };
        let (min, max) = column.limits();
        if value.is_finite() {
            value.clamp(min, max)
        } else {
            Self::default().get(column)
        }
    }

    fn set(&mut self, column: Column, value: f32) {
        match column {
            Column::Name => (),
            Column::Author => self.author = value,
            Column::Size => self.size = value,
            Column::Modified => self.modified = value,
            Column::Status => self.status = value,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct TableLayout {
    widths: [f32; 5],
    visible: [bool; 5],
}

impl TableLayout {
    pub fn new(header_width: f32, preferences: Preferences, widths: ColumnWidths) -> Self {
        let visible = [
            true,
            preferences.author_column,
            true,
            preferences.modified_column,
            true,
        ];
        let mut layout = Self {
            widths: [0.; 5],
            visible,
        };
        // Header and rows share padding, checkbox width and inter-column gaps.
        let available =
            header_width - 24. - widgets::CHECKBOX_WIDTH - 12. * layout.columns().count() as f32;
        let mut minimum = 0.;
        let mut desired = 0.;
        for column in layout.columns().skip(1) {
            minimum += column.limits().0;
            desired += widths.get(column);
        }
        let budget = (available - Column::Name.limits().0).max(minimum);
        let ratio = if desired > budget {
            (budget - minimum) / (desired - minimum)
        } else {
            1.
        };
        let mut used = 0.;
        for column in layout.columns().skip(1) {
            let min = column.limits().0;
            let width = min + (widths.get(column) - min) * ratio;
            layout.widths[column as usize] = width;
            used += width;
        }
        layout.widths[Column::Name as usize] = (available - used).max(Column::Name.limits().0);
        layout
    }

    pub fn columns(self) -> impl Iterator<Item = Column> {
        Column::ALL
            .into_iter()
            .filter(move |column| self.visible[*column as usize])
    }

    pub fn width(self, column: Column) -> f32 {
        self.widths[column as usize]
    }

    fn stored_widths(self, mut stored: ColumnWidths) -> ColumnWidths {
        for column in self.columns() {
            stored.set(column, self.width(column));
        }
        stored
    }
}

#[derive(Clone, Copy)]
pub(super) struct ColumnResize {
    left: Column,
    right: Column,
    start_x: f32,
    initial: TableLayout,
    pub widths: ColumnWidths,
    moved: bool,
}

impl ColumnResize {
    fn new(
        left: Column,
        right: Column,
        start_x: f32,
        initial: TableLayout,
        stored: ColumnWidths,
    ) -> Self {
        Self {
            left,
            right,
            start_x,
            initial,
            widths: initial.stored_widths(stored),
            moved: false,
        }
    }

    fn update(&mut self, x: f32) {
        let left = self.initial.width(self.left);
        let right = self.initial.width(self.right);
        let (left_min, left_max) = self.left.limits();
        let (right_min, right_max) = self.right.limits();
        let lower = (left_min - left).max(right - right_max);
        let upper = (left_max - left).min(right - right_min);
        let delta = (x - self.start_x).clamp(lower, upper);
        self.widths.set(self.left, left + delta);
        self.widths.set(self.right, right - delta);
        self.moved = delta.abs() >= 0.5;
    }
}

impl Manager {
    pub(super) fn table_header(
        &self,
        columns: Preferences,
        layout: TableLayout,
        all_checked: bool,
        checked: usize,
        cx: &Context<Self>,
    ) -> Div {
        div()
            .relative()
            .w_full()
            .min_w_0()
            .h(px(38.))
            .px_3()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap_3()
            .bg(rgb(0xfafbfc))
            .text_xs()
            .text_color(rgb(MUTED))
            .child(self.header_measure(cx))
            .child(self.checkbox(
                "check-all",
                all_checked,
                checked > 0 && !all_checked,
                Command::CheckAll,
                cx,
            ))
            .child(
                self.table_header_cell(
                    Column::Name,
                    layout,
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(self.sort_header("name-column", "名称", SortColumn::Name, cx))
                        .when(
                            self.browser.sort.column() == SortColumn::Author
                                && !columns.author_column
                                || self.browser.sort.column() == SortColumn::Modified
                                    && !columns.modified_column,
                            |el| {
                                el.child(
                                    div()
                                        .min_w_0()
                                        .text_ellipsis()
                                        .overflow_hidden()
                                        .child(self.browser.sort.label()),
                                )
                            },
                        ),
                    cx,
                ),
            )
            .when(columns.author_column, |el| {
                el.child(
                    self.table_header_cell(
                        Column::Author,
                        layout,
                        div().child(
                            self.sort_header("author-column", "作者", SortColumn::Author, cx)
                                .w_full(),
                        ),
                        cx,
                    ),
                )
            })
            .child(
                self.table_header_cell(
                    Column::Size,
                    layout,
                    div().child(
                        self.sort_header("size-column", "大小", SortColumn::Size, cx)
                            .w_full(),
                    ),
                    cx,
                ),
            )
            .when(columns.modified_column, |el| {
                el.child(
                    self.table_header_cell(
                        Column::Modified,
                        layout,
                        div().child(
                            self.sort_header(
                                "modified-column",
                                "文件修改",
                                SortColumn::Modified,
                                cx,
                            )
                            .w_full(),
                        ),
                        cx,
                    ),
                )
            })
            .child(
                self.table_header_cell(
                    Column::Status,
                    layout,
                    div()
                        .when(self.can_manage(), |el| {
                            el.child(
                                self.button(
                                    "status-column",
                                    match self.browser.status {
                                        Status::All => "启用状态 ▾",
                                        Status::Enabled => "已启用 ▾",
                                        Status::Disabled => "已禁用 ▾",
                                    },
                                    Command::Menu(Menu::Status),
                                    false,
                                    true,
                                    cx,
                                )
                                .w_full()
                                .h(px(28.))
                                .px_1()
                                .justify_start()
                                .text_xs()
                                .bg(rgb(if self.browser.status == Status::All {
                                    0xfafbfc
                                } else {
                                    TINT
                                }))
                                .text_color(rgb(
                                    if self.browser.status == Status::All {
                                        MUTED
                                    } else {
                                        ACCENT
                                    },
                                )),
                            )
                        })
                        .when(!self.can_manage(), |el| el.child("适配状态")),
                    cx,
                ),
            )
    }

    pub(super) fn column_layout(&self, columns: Preferences) -> TableLayout {
        TableLayout::new(
            self.table_header_width.unwrap_or(0.),
            columns,
            self.column_resize
                .map(|drag| drag.widths)
                .unwrap_or(self.preferences.column_widths),
        )
    }

    pub(super) fn table_header_cell(
        &self,
        column: Column,
        layout: TableLayout,
        content: Div,
        cx: &Context<Self>,
    ) -> Div {
        div()
            .relative()
            .h_full()
            .w(px(layout.width(column)))
            .flex_shrink_0()
            .flex()
            .items_center()
            .child(content.w_full().min_w_0().overflow_hidden())
            .when_some(
                layout.columns().skip_while(|c| *c != column).nth(1),
                |el, right| {
                    el.child(
                        div()
                            .id(("column-resize", column as usize))
                            .absolute()
                            .right(px(-10.))
                            .top(px(5.))
                            .bottom(px(5.))
                            .w(px(8.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor(gpui::CursorStyle::ResizeLeftRight)
                            .hover(|el| el.bg(rgb(TINT)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(
                                    move |this, event: &gpui::MouseDownEvent, window, cx| {
                                        if this.modal_open() {
                                            return;
                                        }
                                        cx.stop_propagation();
                                        window.prevent_default();
                                        window.focus(&this.focus);
                                        this.menu = None;
                                        this.context_menu = None;
                                        this.column_resize = Some(ColumnResize::new(
                                            column,
                                            right,
                                            event.position.x.into(),
                                            layout,
                                            this.preferences.column_widths,
                                        ));
                                        cx.notify();
                                    },
                                ),
                            )
                            .on_click(|_, _, cx| cx.stop_propagation())
                            .child(div().w(px(1.)).h(px(18.)).bg(rgb(LINE))),
                    )
                },
            )
    }

    pub(super) fn header_measure(&self, cx: &Context<Self>) -> impl IntoElement {
        let entity = cx.entity().downgrade();
        canvas(
            move |bounds, _, cx| {
                let _ = entity.update(cx, |this, cx| {
                    let width = f32::from(bounds.size.width);
                    if this
                        .table_header_width
                        .is_none_or(|old| (old - width).abs() > 0.5)
                    {
                        this.table_header_width = Some(width);
                        // A changed viewport invalidates the drag's starting geometry.
                        this.column_resize = None;
                        cx.notify();
                    }
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
    }

    fn finish_column_resize(&mut self, cx: &mut Context<Self>) {
        if let Some(drag) = self.column_resize.take() {
            if drag.moved {
                let mut updated = self.preferences;
                updated.column_widths = drag.widths;
                self.save_column_preferences(updated);
            }
            cx.notify();
        }
    }

    fn save_column_preferences(&mut self, updated: Preferences) -> bool {
        match updated.save(&discovery::data_dir().join("preferences.json")) {
            Ok(()) => {
                self.preferences = updated;
                if let Some(error) = self.preferences_error.take()
                    && self.message == error
                {
                    self.error = false;
                    self.set_message(String::new());
                }
                true
            }
            Err(error) => {
                self.preferences_error = Some(error.clone());
                self.error = true;
                self.set_message(error);
                false
            }
        }
    }

    pub(super) fn reset_column_widths(&mut self) {
        let mut updated = self.preferences;
        updated.column_widths = ColumnWidths::default();
        if self.save_column_preferences(updated) {
            self.error = false;
            self.set_message("列宽已恢复默认".into());
        }
    }

    pub(super) fn column_resize_overlay(&self, cx: &Context<Self>) -> Stateful<Div> {
        div()
            .id("column-resize-shield")
            .absolute()
            .inset_0()
            .occlude()
            .cursor(gpui::CursorStyle::ResizeLeftRight)
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                cx.stop_propagation();
                if event.dragging() {
                    if let Some(drag) = &mut this.column_resize {
                        drag.update(event.position.x.into());
                        cx.notify();
                    }
                } else {
                    this.finish_column_resize(cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseUpEvent, _, cx| {
                    cx.stop_propagation();
                    if let Some(drag) = &mut this.column_resize {
                        drag.update(event.position.x.into());
                    }
                    this.finish_column_resize(cx);
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    this.finish_column_resize(cx);
                }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_fits(layout: TableLayout, width: f32) {
        let total = layout.columns().map(|c| layout.width(c)).sum::<f32>()
            + 24.
            + 36.
            + 12. * layout.columns().count() as f32;
        assert!((total - width).abs() < 0.01, "{total} != {width}");
        for column in layout.columns() {
            let (min, max) = column.limits();
            assert!(layout.width(column).is_finite());
            assert!(layout.width(column) >= min - 0.01);
            assert!(layout.width(column) <= max);
        }
    }

    #[test]
    fn layouts_fit_available_space_and_preserve_hidden_width_preferences() {
        let stored = ColumnWidths {
            author: 320.,
            size: 180.,
            modified: 260.,
            status: 200.,
        };
        for author in [false, true] {
            for modified in [false, true] {
                let prefs = Preferences {
                    author_column: author,
                    modified_column: modified,
                    column_widths: stored,
                };
                for width in [700., 850., 1200.] {
                    let layout = TableLayout::new(width, prefs, stored);
                    assert_fits(layout, width);
                    let effective = layout.stored_widths(stored);
                    if !author {
                        assert_eq!(effective.author, stored.author);
                    }
                    if !modified {
                        assert_eq!(effective.modified, stored.modified);
                    }
                }
            }
        }
        let prefs = Preferences {
            author_column: true,
            modified_column: true,
            column_widths: stored,
        };
        let narrow = prefs.visible(false);
        assert_eq!(narrow.column_widths, stored);
        assert_fits(TableLayout::new(566., narrow, stored), 566.);
    }

    #[test]
    fn dragging_every_boundary_keeps_columns_aligned_and_within_limits() {
        for prefs in [
            Preferences::default(),
            Preferences {
                author_column: true,
                modified_column: true,
                ..Default::default()
            },
        ] {
            let layout = TableLayout::new(850., prefs, prefs.column_widths);
            let columns = layout.columns().collect::<Vec<_>>();
            for pair in columns.windows(2) {
                let mut drag =
                    ColumnResize::new(pair[0], pair[1], 100., layout, prefs.column_widths);
                for x in [-10000., 60., 140., 10000., 100.] {
                    drag.update(x);
                    let current = TableLayout::new(850., prefs, drag.widths);
                    assert_fits(current, 850.);
                    assert!(
                        (current.width(pair[0]) + current.width(pair[1])
                            - layout.width(pair[0])
                            - layout.width(pair[1]))
                        .abs()
                            < 0.01
                    );
                    for column in columns.iter().copied().filter(|c| !pair.contains(c)) {
                        assert!((current.width(column) - layout.width(column)).abs() < 0.01);
                    }
                }
                assert!(!drag.moved);
            }
        }
    }

    #[test]
    fn invalid_saved_widths_cannot_collapse_columns_or_overflow_the_table() {
        let invalid = ColumnWidths {
            author: f32::NAN,
            size: -50.,
            modified: f32::INFINITY,
            status: 100000.,
        };
        let prefs = Preferences {
            author_column: true,
            modified_column: true,
            column_widths: invalid,
        };
        assert_fits(TableLayout::new(850., prefs, invalid), 850.);
    }
}

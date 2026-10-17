use super::*;

#[derive(Clone, Copy)]
pub(super) enum OverlayKind {
    Modal,
    Menu,
}

/// Use this for every floating surface. Visual backgrounds do not block GPUI hit testing.
pub(super) fn panel(id: impl Into<ElementId>) -> Stateful<Div> {
    div()
        .id(id)
        .occlude()
        .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
        .on_click(|_, _, cx| cx.stop_propagation())
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
}

/// Draw a font-independent close mark with a consistent stroke at every scale.
pub(super) fn close_icon(color: u32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let mut path = gpui::PathBuilder::stroke(px(1.6));
            path.move_to(point(bounds.left() + px(2.), bounds.top() + px(2.)));
            path.line_to(point(bounds.right() - px(2.), bounds.bottom() - px(2.)));
            path.move_to(point(bounds.right() - px(2.), bounds.top() + px(2.)));
            path.line_to(point(bounds.left() + px(2.), bounds.bottom() - px(2.)));
            if let Ok(path) = path.build() {
                window.paint_path(path, rgb(color));
            }
        },
    )
    .size(px(14.))
    .flex_shrink_0()
}

impl Manager {
    pub(super) fn close_button(
        &self,
        id: impl Into<ElementId>,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        self.button(id, "", Command::Close, false, true, cx)
            .w(px(34.))
            .px_0()
            .rounded(px(6.))
            .child(close_icon(TEXT))
    }

    pub(super) fn modal_open(&self) -> bool {
        self.column_resize.is_some()
            || self.group_editor.is_some()
            || self.group_toggle.is_some()
            || self.missing_group.is_some()
            || self.detail.is_some()
            || self.page != Page::Addons
    }

    fn dismiss_overlay(&mut self, kind: OverlayKind, window: &mut Window, cx: &mut Context<Self>) {
        match kind {
            OverlayKind::Modal => self.command(Command::Close, cx),
            OverlayKind::Menu => {
                self.menu = None;
                self.context_menu = None;
                cx.notify();
            }
        }
        window.focus(&self.focus);
    }

    /// Consume outside input before removing the overlay, so dismissal never activates the view below.
    pub(super) fn overlay_backdrop(
        &self,
        id: impl Into<ElementId>,
        kind: OverlayKind,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        div()
            .id(id)
            .absolute()
            .inset_0()
            .occlude()
            .on_any_mouse_down(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.dismiss_overlay(kind, window, cx);
            }))
            .on_scroll_wheel(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                // Scrolling outside an editor must not discard an unfinished name.
                if matches!(kind, OverlayKind::Menu) {
                    this.dismiss_overlay(kind, window, cx);
                }
            }))
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_only_allow_own_navigation_data_and_close() {
        for command in [
            Command::Close,
            Command::Navigate(Page::Settings),
            Command::DataFolder,
            Command::Refresh,
            Command::ToggleColumn(OptionalColumn::Author),
            Command::ToggleColumn(OptionalColumn::Modified),
            Command::ResetColumnWidths,
        ] {
            assert!(command.allowed_in_settings());
        }
        for command in [
            Command::Import,
            Command::LaunchGame,
            Command::CheckAll,
            Command::Bulk(false),
            Command::GroupToggle(0, true),
            Command::Browse(None, Status::All),
            Command::Navigate(Page::Addons),
            Command::CreateEmptyGroup,
        ] {
            assert!(!command.allowed_in_settings());
        }
    }

    #[test]
    fn details_block_background_navigation_selection_and_writes() {
        let blocked = [
            Command::ToggleColumn(OptionalColumn::Author),
            Command::Import,
            Command::LaunchGame,
            Command::Refresh,
            Command::CheckAll,
            Command::Check("other.vpk".into()),
            Command::Bulk(false),
            Command::GroupToggle(0, false),
            Command::Browse(None, Status::All),
            Command::Navigate(Page::Settings),
            Command::Menu(Menu::Games),
            Command::CreateEmptyGroup,
            Command::Toggle("other.vpk".into(), false),
            Command::FetchWorkshop("other.vpk".into()),
            Command::AssociateWorkshop("other.vpk".into()),
        ];
        for command in blocked {
            assert!(!command.allowed_in_details(Some("shown.vpk")));
        }
    }

    #[test]
    fn details_keep_close_metadata_and_the_displayed_addons_controls_available() {
        let allowed = [
            Command::Close,
            Command::TechnicalDetails,
            Command::EditWorkshopLink,
            Command::Reveal(PathBuf::from("shown.vpk")),
            Command::CopyPath(PathBuf::from("shown.vpk")),
            Command::WorkshopPage("123".into()),
            Command::WorkshopBrowser("123".into()),
            Command::FetchWorkshop("shown.vpk".into()),
            Command::AssociateWorkshop("shown.vpk".into()),
            Command::Toggle("shown.vpk".into(), false),
        ];
        for command in allowed {
            assert!(command.allowed_in_details(Some("shown.vpk")));
        }
    }
}

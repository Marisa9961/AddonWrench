use super::*;

/// Monochrome glyphs from the Segoe MDL2 Assets font that ships with Windows.
///
/// Keeping every action on the same icon font gives the toolbar, dialogs and
/// context menus one consistent visual language without shipping image assets.
#[derive(Clone, Copy)]
pub(super) enum Icon {
    Play,
    Add,
    Folder,
    FolderOpen,
    Settings,
    Search,
    Clear,
    Cancel,
    Refresh,
    Tag,
    Filter,
    Copy,
    OpenInNew,
    Link,
    Sync,
    Warning,
    Info,
    Document,
    Rename,
    Delete,
    Save,
    Check,
    Block,
    Back,
    NewFolder,
    AddTo,
    RemoveFrom,
    Remove,
    HardDrive,
    Reset,
    ToggleOn,
    ToggleOff,
    List,
    View,
}

impl Icon {
    fn glyph(self) -> &'static str {
        match self {
            Self::Play => "\u{e768}",
            Self::Add => "\u{e710}",
            Self::Folder => "\u{e8b7}",
            Self::FolderOpen => "\u{e838}",
            Self::Settings => "\u{e713}",
            Self::Search => "\u{e721}",
            Self::Clear => "\u{e894}",
            Self::Cancel => "\u{e711}",
            Self::Refresh => "\u{e72c}",
            Self::Tag => "\u{e8ec}",
            Self::Filter => "\u{e71c}",
            Self::Copy => "\u{e8c8}",
            Self::OpenInNew => "\u{e8a7}",
            Self::Link => "\u{e71b}",
            Self::Sync => "\u{e895}",
            Self::Warning => "\u{e7ba}",
            Self::Info => "\u{e946}",
            Self::Document => "\u{e8a5}",
            Self::Rename => "\u{e8ac}",
            Self::Delete => "\u{e74d}",
            Self::Save => "\u{e74e}",
            Self::Check => "\u{e8fb}",
            Self::Block => "\u{e8f8}",
            Self::Back => "\u{e72b}",
            Self::NewFolder => "\u{e8f4}",
            Self::AddTo => "\u{ecc8}",
            Self::RemoveFrom => "\u{ecc9}",
            Self::Remove => "\u{e738}",
            Self::HardDrive => "\u{eda2}",
            Self::Reset => "\u{e777}",
            Self::ToggleOn => "\u{f19f}",
            Self::ToggleOff => "\u{f19e}",
            Self::List => "\u{e8fd}",
            Self::View => "\u{e8a9}",
        }
    }
}

/// A fixed-size leading glyph that inherits its parent's text color.
pub(super) fn leading(icon: Icon) -> Div {
    div()
        .flex_shrink_0()
        .w(px(16.))
        .h(px(16.))
        .flex()
        .items_center()
        .justify_center()
        .font_family("Segoe MDL2 Assets")
        .font_weight(FontWeight::NORMAL)
        .text_size(px(14.))
        .line_height(px(16.))
        .child(icon.glyph())
}

impl Command {
    /// The visual hint shown before a command label. Sorting and selection
    /// controls keep their own text markers, so they intentionally map to
    /// `None`.
    pub(super) fn icon(&self) -> Option<Icon> {
        Some(match self {
            Self::LaunchGame => Icon::Play,
            Self::Choose => Icon::NewFolder,
            Self::Import => Icon::Add,
            Self::Folder => Icon::Folder,
            Self::DataFolder => return None,
            Self::Refresh => Icon::Refresh,
            Self::Navigate(Page::Settings) => Icon::Settings,
            Self::Navigate(Page::Addons) => Icon::List,
            Self::SwitchGame(_) => return None,
            Self::Menu(Menu::Games) => return None,
            Self::Menu(Menu::Tags) => Icon::Tag,
            Self::Menu(Menu::Status) => Icon::Filter,
            Self::Tag(_) | Self::Status(_) => return None,
            Self::Menu(Menu::AddToGroup) => Icon::AddTo,
            Self::Menu(Menu::Groups(_)) => return None,
            Self::CreateEmptyGroup | Self::CreateGroupWith(_) => Icon::NewFolder,
            Self::Browse(_, _) => Icon::List,
            Self::EditGroup(_) => Icon::Rename,
            Self::SaveGroup => Icon::Save,
            Self::DeleteGroup(_) => Icon::Delete,
            Self::GroupToggle(_, enabled) => {
                if *enabled {
                    Icon::ToggleOn
                } else {
                    Icon::ToggleOff
                }
            }
            Self::RemoveFromGroup | Self::RemoveKeysFromGroup(_, _) => Icon::RemoveFrom,
            Self::AddKeysToGroup(_, _) | Self::ContextAddToGroup => Icon::AddTo,
            Self::ClearMissingGroup => Icon::Remove,
            Self::MissingGroup(_) => Icon::Warning,
            Self::ConfirmGroupToggle => Icon::Check,
            Self::Bulk(enabled) | Self::Toggle(_, enabled) => {
                if *enabled {
                    Icon::Check
                } else {
                    Icon::Block
                }
            }
            Self::ClearSelection | Self::ClearSearch => Icon::Clear,
            Self::ResetFilters => Icon::Reset,
            Self::Reveal(_) => Icon::FolderOpen,
            Self::CopyPath(_) => Icon::Copy,
            Self::Detail(_) => Icon::Info,
            Self::TechnicalDetails => Icon::Document,
            Self::EditWorkshopLink => Icon::Link,
            Self::FetchWorkshop(_) | Self::AssociateWorkshop(_) => Icon::Sync,
            Self::WorkshopPage(_) | Self::WorkshopBrowser(_) => Icon::OpenInNew,
            Self::Close | Self::DismissMessage => Icon::Cancel,
            Self::ContextBack => Icon::Back,
            Self::ToggleColumn(_) => Icon::View,
            Self::ResetColumnWidths => Icon::Reset,
            Self::Check(_) | Self::CheckAll | Self::Sort(_) => return None,
        })
    }
}

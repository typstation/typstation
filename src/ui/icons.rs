//! Ícones de fluxo de trabalho usados pelos componentes Spectrum.

use std::sync::LazyLock;

use iced::widget::svg;

const TEXT_BOLD: &[u8] = include_bytes!("assets/text-bold.svg");
const TEXT_ITALIC: &[u8] = include_bytes!("assets/text-italic.svg");
const TEXT_UNDERLINE: &[u8] = include_bytes!("assets/text-underline.svg");
const TEXT_BULLETED: &[u8] = include_bytes!("assets/text-bulleted.svg");
const TEXT_NUMBERED: &[u8] = include_bytes!("assets/text-numbered.svg");
const CHEVRON_DOWN: &[u8] = include_bytes!("assets/chevron-down.svg");
const CHEVRON_RIGHT: &[u8] = include_bytes!("assets/chevron-right.svg");
const DOCUMENT: &[u8] = include_bytes!("assets/document.svg");
const FILE_CODE: &[u8] = include_bytes!("assets/file-code.svg");
const FOLDER: &[u8] = include_bytes!("assets/folder.svg");
const FOLDER_OPEN: &[u8] = include_bytes!("assets/folder-open.svg");
const PROJECT: &[u8] = include_bytes!("assets/project.svg");
const FILE_ADD: &[u8] = include_bytes!("assets/file-add.svg");
const FOLDER_ADD: &[u8] = include_bytes!("assets/folder-add.svg");
const REFRESH: &[u8] = include_bytes!("assets/refresh.svg");
const VISIBILITY: &[u8] = include_bytes!("assets/visibility.svg");
const ALERT: &[u8] = include_bytes!("assets/alert.svg");
const ALERT_CIRCLE_FILLED: &[u8] = include_bytes!("assets/alert-circle-filled.svg");
const SEARCH: &[u8] = include_bytes!("assets/search.svg");
const FIND_AND_REPLACE: &[u8] = include_bytes!("assets/find-and-replace.svg");
const CHEVRON_UP: &[u8] = include_bytes!("assets/chevron-up.svg");
const CLOSE: &[u8] = include_bytes!("assets/close.svg");
const SETTINGS: &[u8] = include_bytes!("assets/settings.svg");
const MORE: &[u8] = include_bytes!("assets/more.svg");
const UNDO: &[u8] = include_bytes!("assets/undo.svg");
const REDO: &[u8] = include_bytes!("assets/redo.svg");
const CODE: &[u8] = include_bytes!("assets/code.svg");
const ZOOM_IN: &[u8] = include_bytes!("assets/zoom-in.svg");
const ZOOM_OUT: &[u8] = include_bytes!("assets/zoom-out.svg");
const CROSS_100: &[u8] = include_bytes!("assets/cross-100.svg");
const CHECKMARK_100: &[u8] = include_bytes!("assets/checkmark-100.svg");

static HANDLES: LazyLock<[svg::Handle; 29]> = LazyLock::new(|| {
    [
        svg::Handle::from_memory(TEXT_BOLD),
        svg::Handle::from_memory(TEXT_ITALIC),
        svg::Handle::from_memory(TEXT_UNDERLINE),
        svg::Handle::from_memory(TEXT_BULLETED),
        svg::Handle::from_memory(TEXT_NUMBERED),
        svg::Handle::from_memory(CHEVRON_DOWN),
        svg::Handle::from_memory(CHEVRON_RIGHT),
        svg::Handle::from_memory(DOCUMENT),
        svg::Handle::from_memory(FILE_CODE),
        svg::Handle::from_memory(FOLDER),
        svg::Handle::from_memory(FOLDER_OPEN),
        svg::Handle::from_memory(PROJECT),
        svg::Handle::from_memory(FILE_ADD),
        svg::Handle::from_memory(FOLDER_ADD),
        svg::Handle::from_memory(REFRESH),
        svg::Handle::from_memory(VISIBILITY),
        svg::Handle::from_memory(ALERT),
        svg::Handle::from_memory(ALERT_CIRCLE_FILLED),
        svg::Handle::from_memory(SEARCH),
        svg::Handle::from_memory(FIND_AND_REPLACE),
        svg::Handle::from_memory(CHEVRON_UP),
        svg::Handle::from_memory(CLOSE),
        svg::Handle::from_memory(SETTINGS),
        svg::Handle::from_memory(MORE),
        svg::Handle::from_memory(UNDO),
        svg::Handle::from_memory(REDO),
        svg::Handle::from_memory(CODE),
        svg::Handle::from_memory(ZOOM_IN),
        svg::Handle::from_memory(ZOOM_OUT),
    ]
});

static UI_HANDLES: LazyLock<[svg::Handle; 2]> = LazyLock::new(|| {
    [
        svg::Handle::from_memory(CROSS_100),
        svg::Handle::from_memory(CHECKMARK_100),
    ]
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowIcon {
    TextBold,
    TextItalic,
    TextUnderline,
    TextBulleted,
    TextNumbered,
    ChevronDown,
    ChevronRight,
    Document,
    FileCode,
    Folder,
    FolderOpen,
    Project,
    FileAdd,
    FolderAdd,
    Refresh,
    Visibility,
    Alert,
    AlertCircleFilled,
    Search,
    FindAndReplace,
    ChevronUp,
    Close,
    Settings,
    More,
    Undo,
    Redo,
    Code,
    ZoomIn,
    ZoomOut,
}

impl WorkflowIcon {
    pub fn handle(self) -> svg::Handle {
        HANDLES[self.index()].clone()
    }

    const fn index(self) -> usize {
        match self {
            Self::TextBold => 0,
            Self::TextItalic => 1,
            Self::TextUnderline => 2,
            Self::TextBulleted => 3,
            Self::TextNumbered => 4,
            Self::ChevronDown => 5,
            Self::ChevronRight => 6,
            Self::Document => 7,
            Self::FileCode => 8,
            Self::Folder => 9,
            Self::FolderOpen => 10,
            Self::Project => 11,
            Self::FileAdd => 12,
            Self::FolderAdd => 13,
            Self::Refresh => 14,
            Self::Visibility => 15,
            Self::Alert => 16,
            Self::AlertCircleFilled => 17,
            Self::Search => 18,
            Self::FindAndReplace => 19,
            Self::ChevronUp => 20,
            Self::Close => 21,
            Self::Settings => 22,
            Self::More => 23,
            Self::Undo => 24,
            Self::Redo => 25,
            Self::Code => 26,
            Self::ZoomIn => 27,
            Self::ZoomOut => 28,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiIcon {
    Cross100,
    Checkmark100,
}

impl UiIcon {
    pub fn handle(self) -> svg::Handle {
        UI_HANDLES[self.index()].clone()
    }

    const fn index(self) -> usize {
        match self {
            Self::Cross100 => 0,
            Self::Checkmark100 => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_workflow_icon_has_a_distinct_cached_handle() {
        let icons = [
            WorkflowIcon::TextBold,
            WorkflowIcon::TextItalic,
            WorkflowIcon::TextUnderline,
            WorkflowIcon::TextBulleted,
            WorkflowIcon::TextNumbered,
            WorkflowIcon::ChevronDown,
            WorkflowIcon::ChevronRight,
            WorkflowIcon::Document,
            WorkflowIcon::FileCode,
            WorkflowIcon::Folder,
            WorkflowIcon::FolderOpen,
            WorkflowIcon::Project,
            WorkflowIcon::FileAdd,
            WorkflowIcon::FolderAdd,
            WorkflowIcon::Refresh,
            WorkflowIcon::Visibility,
            WorkflowIcon::Alert,
            WorkflowIcon::AlertCircleFilled,
            WorkflowIcon::Search,
            WorkflowIcon::FindAndReplace,
            WorkflowIcon::ChevronUp,
            WorkflowIcon::Close,
            WorkflowIcon::Settings,
            WorkflowIcon::More,
            WorkflowIcon::Undo,
            WorkflowIcon::Redo,
            WorkflowIcon::Code,
            WorkflowIcon::ZoomIn,
            WorkflowIcon::ZoomOut,
        ];
        let ids: Vec<_> = icons.into_iter().map(|icon| icon.handle().id()).collect();

        for (position, id) in ids.iter().enumerate() {
            assert!(!ids[..position].contains(id));
        }
    }

    #[test]
    fn every_ui_icon_has_a_cached_handle() {
        for icon in [UiIcon::Cross100, UiIcon::Checkmark100] {
            let first = icon.handle();
            let second = icon.handle();

            assert_eq!(first.id(), second.id());
        }
    }
}

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
const PREVIEW: &[u8] = include_bytes!("assets/preview.svg");
const CROSS_100: &[u8] = include_bytes!("assets/cross-100.svg");
const CHECKMARK_100: &[u8] = include_bytes!("assets/checkmark-100.svg");

static HANDLES: LazyLock<[svg::Handle; 16]> = LazyLock::new(|| {
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
        svg::Handle::from_memory(PREVIEW),
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
    Preview,
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
            Self::Preview => 15,
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
            WorkflowIcon::Preview,
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

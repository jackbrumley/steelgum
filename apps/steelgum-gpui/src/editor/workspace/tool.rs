#[derive(Clone, Copy, PartialEq)]
pub(in crate::editor) enum Tool {
    Objects,
    Assets,
    Changes,
    Ui,
    Build,
}

impl Tool {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Objects => "Objects",
            Self::Assets => "Assets",
            Self::Changes => "Changes",
            Self::Ui => "UI",
            Self::Build => "Build Game",
        }
    }

    pub(super) fn icon(self) -> &'static str {
        match self {
            Self::Objects => "◇",
            Self::Assets => "▦",
            Self::Changes => "⑂",
            Self::Ui => "▣",
            Self::Build => "⚒",
        }
    }
}

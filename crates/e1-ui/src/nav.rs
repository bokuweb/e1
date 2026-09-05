//! What the sidebar offers, and what the centre column is showing.

use e1_github::{ListKind, RepoId, StatusFilter};

/// The fixed rows at the top of the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    /// Unread notifications.
    Inbox,
    /// Open pulls the viewer authored.
    MyPulls,
    /// Open pulls waiting for the viewer's review.
    Reviews,
    /// Open issues assigned to the viewer.
    Assigned,
}

impl Section {
    /// All four, in sidebar order.
    pub const ALL: &'static [Section] = &[
        Section::Inbox,
        Section::MyPulls,
        Section::Reviews,
        Section::Assigned,
    ];

    /// The locale key for the row's label.
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Inbox => "sidebar.inbox",
            Self::MyPulls => "sidebar.my_pulls",
            Self::Reviews => "sidebar.reviews",
            Self::Assigned => "sidebar.assigned",
        }
    }

    /// The row's icon, as an asset path.
    pub fn icon(self) -> &'static str {
        use crate::assets::icon;
        match self {
            Self::Inbox => icon::INBOX,
            Self::MyPulls => icon::PULL_REQUEST,
            Self::Reviews => icon::EYE,
            Self::Assigned => icon::USER_CHECK,
        }
    }

    /// The search that lists the section, or `None` for the inbox, which
    /// is its own endpoint. `@me` rather than the login, so the query is
    /// the same one a person would type into GitHub.
    pub fn query(self) -> Option<&'static str> {
        match self {
            Self::Inbox => None,
            Self::MyPulls => Some("is:pr is:open author:@me"),
            Self::Reviews => Some("is:pr is:open review-requested:@me"),
            Self::Assigned => Some("is:issue is:open assignee:@me"),
        }
    }
}

/// What the centre column lists.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Focus {
    /// One of the fixed sections.
    Section(Section),
    /// A repository's pulls or issues.
    Repo {
        /// Which repository.
        repo: RepoId,
        /// Pulls or issues.
        kind: ListKind,
        /// Open, closed, or all.
        status: StatusFilter,
    },
}

impl Focus {
    /// A repository's open pulls, which is what picking a repository shows
    /// first.
    pub fn repo(repo: RepoId) -> Self {
        Self::Repo {
            repo,
            kind: ListKind::Pulls,
            status: StatusFilter::Open,
        }
    }

    /// The same focus with another kind, keeping the repository.
    pub fn with_kind(&self, kind: ListKind) -> Option<Self> {
        match self {
            Self::Repo { repo, status, .. } => Some(Self::Repo {
                repo: repo.clone(),
                kind,
                status: *status,
            }),
            Self::Section(_) => None,
        }
    }

    /// The same focus with another status, keeping the repository.
    pub fn with_status(&self, status: StatusFilter) -> Option<Self> {
        match self {
            Self::Repo { repo, kind, .. } => Some(Self::Repo {
                repo: repo.clone(),
                kind: *kind,
                status,
            }),
            Self::Section(_) => None,
        }
    }

    /// The repository, when the focus is one.
    pub fn repo_id(&self) -> Option<&RepoId> {
        match self {
            Self::Repo { repo, .. } => Some(repo),
            Self::Section(_) => None,
        }
    }

    /// What the centre strip says.
    pub fn title(&self) -> String {
        match self {
            Self::Section(section) => rust_i18n::t!(section.label_key()).to_string(),
            Self::Repo { repo, .. } => repo.to_string(),
        }
    }

    /// What the centre strip says after the title, when there is more.
    pub fn subtitle(&self) -> Option<String> {
        match self {
            Self::Section(_) => None,
            Self::Repo { kind, .. } => Some(
                rust_i18n::t!(match kind {
                    ListKind::Pulls => "list.pulls",
                    ListKind::Issues => "list.issues",
                })
                .to_string(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_section_but_the_inbox_is_a_search() {
        assert_eq!(Section::Inbox.query(), None);
        for section in Section::ALL.iter().filter(|s| **s != Section::Inbox) {
            let query = section.query().unwrap();
            assert!(query.contains("is:open"), "{section:?}: {query}");
            assert!(query.contains("@me"), "{section:?}: {query}");
        }
    }

    #[test]
    fn picking_a_repository_shows_its_open_pulls_first() {
        let focus = Focus::repo(RepoId::new("o", "r"));
        assert_eq!(
            focus,
            Focus::Repo {
                repo: RepoId::new("o", "r"),
                kind: ListKind::Pulls,
                status: StatusFilter::Open
            }
        );
        let issues = focus.with_kind(ListKind::Issues).unwrap();
        assert_eq!(issues.repo_id(), Some(&RepoId::new("o", "r")));
        let closed = issues.with_status(StatusFilter::Closed).unwrap();
        assert!(matches!(
            closed,
            Focus::Repo {
                kind: ListKind::Issues,
                status: StatusFilter::Closed,
                ..
            }
        ));
    }

    #[test]
    fn a_section_has_no_kind_to_switch() {
        assert_eq!(
            Focus::Section(Section::Inbox).with_kind(ListKind::Issues),
            None
        );
        assert_eq!(Focus::Section(Section::Inbox).repo_id(), None);
    }

    #[test]
    fn the_strip_says_what_the_list_is() {
        rust_i18n::set_locale("en");
        assert_eq!(Focus::Section(Section::Reviews).title(), "Reviews");
        let focus = Focus::repo(RepoId::new("bokuweb", "e1"));
        assert_eq!(focus.title(), "bokuweb/e1");
        assert_eq!(focus.subtitle().as_deref(), Some("Pull requests"));
    }
}

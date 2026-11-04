//! Editorial markers apply to authored prose within the selected genre.

pub const MIN_DETAILED_REPORT_CHARACTERS: usize = 15_000;

#[must_use]
pub const fn report_length_review(detailed: bool, authored_characters: usize) -> bool {
    detailed && authored_characters < MIN_DETAILED_REPORT_CHARACTERS
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Context {
    Authored,
    Quotation,
    Metadata,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    Flow,
    List,
    Table,
}

#[must_use]
pub const fn layout_review(context: Context, layout: Layout) -> bool {
    matches!(context, Context::Authored) && matches!(layout, Layout::List | Layout::Table)
}

#[must_use]
pub const fn marker_review(
    matched: bool,
    expository: bool,
    context: Context,
    genre_specific: bool,
) -> bool {
    matched && matches!(context, Context::Authored) && (!genre_specific || expository)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authored_layouts_follow_house_policy_without_changing_literals() {
        for context in [Context::Authored, Context::Quotation, Context::Metadata] {
            for layout in [Layout::Flow, Layout::List, Layout::Table] {
                assert_eq!(
                    layout_review(context, layout),
                    context == Context::Authored && layout != Layout::Flow
                );
            }
        }
    }

    #[test]
    fn genre_and_provenance_preserve_intentional_voice() {
        assert!(marker_review(true, true, Context::Authored, true));
        assert!(!marker_review(true, false, Context::Authored, true));
        assert!(marker_review(true, false, Context::Authored, false));
        assert!(!marker_review(true, true, Context::Quotation, false));
        assert!(!marker_review(true, true, Context::Metadata, false));
        assert!(!marker_review(false, true, Context::Authored, false));
    }
}

#[cfg(kani)]
mod proofs {
    use super::*;

    #[kani::proof]
    fn detailed_report_length_respects_the_exact_boundary() {
        let characters: usize = kani::any();
        if characters >= MIN_DETAILED_REPORT_CHARACTERS {
            assert!(!report_length_review(true, characters));
        } else {
            assert!(report_length_review(true, characters));
        }
        kani::cover!(characters == MIN_DETAILED_REPORT_CHARACTERS);
        kani::cover!(characters == MIN_DETAILED_REPORT_CHARACTERS - 1);
    }

    #[kani::proof]
    fn ordinary_prose_has_no_detailed_report_length_requirement() {
        let characters: usize = kani::any();
        assert!(!report_length_review(false, characters));
        kani::cover!(characters == 0);
        kani::cover!(characters == usize::MAX);
    }

    #[kani::proof]
    fn layout_policy_covers_every_context_and_layout() {
        let context = match kani::any::<u8>() % 3 {
            0 => Context::Authored,
            1 => Context::Quotation,
            _ => Context::Metadata,
        };
        let layout = match kani::any::<u8>() % 3 {
            0 => Layout::Flow,
            1 => Layout::List,
            _ => Layout::Table,
        };
        assert_eq!(
            layout_review(context, layout),
            context == Context::Authored && layout != Layout::Flow
        );
        kani::cover!(layout_review(context, layout));
        kani::cover!(!layout_review(context, layout));
    }

    #[kani::proof]
    fn quoted_and_metadata_text_never_trigger_authored_markers() {
        let matched: bool = kani::any();
        let expository: bool = kani::any();
        let specific: bool = kani::any();
        assert!(!marker_review(
            matched,
            expository,
            Context::Quotation,
            specific
        ));
        assert!(!marker_review(
            matched,
            expository,
            Context::Metadata,
            specific
        ));
        kani::cover!(marker_review(
            matched,
            expository,
            Context::Authored,
            specific
        ));
        kani::cover!(!marker_review(
            matched,
            expository,
            Context::Quotation,
            specific
        ));
    }

    #[kani::proof]
    fn general_genre_excludes_expository_markers() {
        let matched: bool = kani::any();
        assert!(!marker_review(matched, false, Context::Authored, true));
        kani::cover!(marker_review(matched, true, Context::Authored, true));
        kani::cover!(!marker_review(matched, false, Context::Authored, true));
    }

    #[kani::proof]
    fn authored_matches_follow_the_selected_scope() {
        let matched: bool = kani::any();
        let expository: bool = kani::any();
        let specific: bool = kani::any();
        assert_eq!(
            marker_review(matched, expository, Context::Authored, specific),
            matched && (expository || !specific)
        );
        kani::cover!(marker_review(true, true, Context::Authored, true));
        kani::cover!(!marker_review(false, true, Context::Authored, true));
    }
}

//! The conformance suite: every implementation of `Screen` runs the same
//! tests, so that the emulator can be swapped by changing a Cargo feature.

// only once build.rs built the library, which a stub build does not
#![cfg(ghostty_built)]

mod paint;
mod queries;
mod round_trip;
mod scrollback;
mod signals;
mod support;
mod widths;

use pinrail_screen::{Options, Screen};

/// Creates a screen of the implementation under test.
pub type New = fn(&Options) -> Box<dyn Screen>;

/// A `#[test]` for each test of the suite, in a module named after the
/// implementation.
macro_rules! conformance {
    ($implementation:ident, $new:expr) => {
        conformance!(@tests $implementation, $new, [
            paint::the_first_paint_rebuilds_the_screen_cursor_and_title,
            paint::the_first_paint_starts_by_cancelling_and_resetting,
            paint::the_first_paint_pushes_the_history_asked_for_into_the_scrollback,
            paint::the_first_paint_shows_the_alternate_screen_over_the_primary_one,
            paint::synchronized_output_left_on_does_not_freeze_the_viewer,
            paint::a_paint_taken_mid_sequence_ends_with_it,
            paint::styles_hyperlinks_and_input_modes_survive_the_first_paint,
            queries::queries_are_answered_in_the_order_they_came,
            queries::the_terminal_names_itself_pinrail,
            queries::size_reports_give_the_terminal_s_size,
            queries::the_colour_scheme_is_reported_once_known,
            queries::colour_queries_are_answered_from_the_display_facts,
            queries::mode_and_keyboard_queries_are_answered,
            queries::the_program_status_probe_is_answered,
            round_trip::random_output_survives_the_first_paint,
            scrollback::history_is_kept_up_to_about_the_line_limit,
            scrollback::a_wide_terminal_stops_at_the_byte_limit,
            scrollback::resizing_changes_the_size,
            signals::a_program_status_report_is_a_signal,
            signals::the_title_bell_and_notifications_are_signals,
            signals::turning_focus_reporting_on_or_off_is_a_signal,
            widths::emoji_with_vs16_take_two_cells_even_after_a_reset,
            widths::zwj_sequences_take_two_cells,
            widths::cjk_takes_two_cells,
        ]);
    };
    (@tests $implementation:ident, $new:expr, [$($module:ident::$test:ident),* $(,)?]) => {
        mod $implementation {
            const NEW: crate::New = $new;
            $(
                #[test]
                fn $test() {
                    crate::$module::$test(NEW)
                }
            )*
        }
    };
}

conformance!(ghostty, |options| Box::new(
    pinrail_screen::ghostty::Terminal::new(options).unwrap()
));

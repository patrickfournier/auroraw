# Interface port: parity checklist (Slint to Qt Quick, milestone Q6)

Every one of the 54 headless scenarios of the Slint shell (`crates/ui/src/headless_tests.rs`) has a
counterpart in the Qt Quick interface's own suites (`crates/ui/tests/qml/tst_*.qml`, run by
`crates/ui/tests/qml.rs`, offscreen, with real key and mouse events), or in a Rust unit test where the
rule is pure. This page is the checklist of the port (D-094), kept as its record: the Slint scenarios can be read at the
tag `slint-shell-final` (`crates/ui/src/headless_tests.rs`).
"same name" means the Qt test carries the Slint scenario's name.

Beyond the 54: the modal guard against keyboard shortcuts, the Alt mnemonics that follow the language,
shortcuts written the way the platform writes them, thumbnails through the asynchronous provider and the
"No preview" cell, the filter bar, the selection kept across reloads, a rating that does not flicker back,
the restore question closed with Escape, the application's own start-up saying nothing about its QML, and
the translation files (`tests/translations.rs`: every string, every plural form, no unfinished entry).

| # | Slint scenario | Qt counterpart |
|---|----------------|----------------|
| 1 | `a_workspace_that_is_empty_opens_on_what_fills_it_and_one_with_photos_on_the_grid` | tst_import: a_workspace_that_is_empty_opens_on_what_fills_it_and_one_with_photos_on_the_grid |
| 2 | `filling_the_form_and_clicking_import_copies_verifies_and_shows_the_photos` | tst_import: filling_the_form_and_clicking_import_copies_verifies_and_shows_the_photos |
| 3 | `the_import_dialog_cannot_be_closed_while_an_import_runs` | tst_import: the_dialog_cannot_be_closed_while_an_import_runs |
| 4 | `a_second_import_of_the_same_card_says_so_and_copies_nothing` | tst_import: same name |
| 5 | `an_import_that_cannot_start_says_why_and_starts_nothing` | tst_import: same name |
| 6 | `the_form_is_remembered_the_next_time_the_workspace_opens` | tst_import: same name |
| 7 | `rating_from_the_keyboard_reaches_the_catalogue` | tst_grid: a_rating_key_reaches_the_cell_the_strip_and_the_catalogue |
| 8 | `page_up_page_down_home_and_end_move_the_selection_and_keep_it_in_view` | tst_grid: paging_keys_home_and_end_move_the_selection_and_keep_it_in_view (+ arrows, short last row, `gridmath` unit tests) |
| 9 | `a_resized_window_shows_as_many_columns_as_fit_and_keeps_the_selected_photo` | tst_grid: same name |
| 10 | `run_time_sentences_are_translated_with_their_plural_forms` | tst_grid: the_sentences_follow_the_language_with_their_plural_forms (+ tests/translations.rs for every plural form) |
| 11 | `the_views_render_and_can_be_written_out_as_pictures` | tst_views: every view, both languages, PNG with AUR_SNAPSHOT_DIR |
| 12 | `browsing_fills_the_field_and_opens_the_dialog_where_the_field_points` | tst_import: the_folder_pickers_fill_their_field_and_open_where_it_points |
| 13 | `while_the_folder_dialog_is_open_the_window_belongs_to_it_and_waits` | tst_dialogs: a_folder_dialog_blocks_the_window_behind_it |
| 14 | `cancelling_the_dialog_leaves_the_field_alone` | tst_import: the_folder_pickers_… (rejected()) |
| 15 | `while_a_dialog_is_open_another_one_cannot_be_started` | tst_dialogs: the_guard_stops_every_command_that_opens_a_window_while_a_dialog_is_open (+ a_click_behind_a_modal_dialog_does_nothing) |
| 16 | `the_dialog_opens_at_the_closest_folder_that_exists` | folders::tests (unit) and tst_import: the_folder_pickers_… |
| 17 | `a_first_launch_offers_to_create_or_open_a_workspace` | tst_launch: same name |
| 18 | `creating_a_workspace_from_the_dialog_opens_it_and_remembers_it` | tst_launch: same name |
| 19 | `the_preview_follows_the_name_and_the_folder_and_the_folder_is_left_alone` | tst_launch: same name |
| 20 | `a_taken_name_is_offered_with_a_number` | tst_launch: same name |
| 21 | `a_workspace_that_cannot_be_created_says_why_and_the_dialog_stays` | tst_launch: same name |
| 22 | `a_typed_folder_is_shown_back_canonical` | tst_launch: a_typed_folder_is_understood_as_its_canonical_form |
| 23 | `the_last_workspace_reopens_on_the_next_launch` | tst_launch: same name |
| 24 | `a_last_workspace_that_cannot_be_found_leads_back_to_the_welcome_list` | tst_launch: same name |
| 25 | `a_known_workspace_opens_from_the_list` | tst_launch: same name |
| 26 | `a_workspace_opens_from_a_folder_chosen_in_the_dialog_and_a_plain_folder_is_refused` | tst_launch: same name |
| 27 | `the_hamburger_menu_opens_lists_its_sections_and_runs_a_command` | tst_menus: same name |
| 28 | `the_menus_group_their_items_with_separators_where_they_are_usually_found` | tst_menus: same name |
| 29 | `importing_is_available_in_the_menu_once_a_workspace_is_open` | tst_menus: importing_is_available_once_a_workspace_is_open_and_no_dialog_is |
| 30 | `every_command_of_the_menu_is_carried_out_by_the_interface` | commands.rs: every_menu_command_is_an_action_with_its_shortcut and the_menu_lists_exactly_the_menu_commands (+ tst_menus: every_command_shows_its_shortcut…) |
| 31 | `the_shortcuts_reach_the_same_commands_as_the_menu` | tst_menus: same name (+ the_alt_keys_open_the_menus_sections) |
| 32 | `escape_closes_the_menu_and_the_dialogs` | tst_menus: same name |
| 33 | `the_edit_menu_acts_on_the_text_field_that_has_the_keyboard` | tst_menus: the_edit_commands_act_on_the_text_field_that_has_the_keyboard |
| 34 | `the_language_is_chosen_in_settings_and_remembered` | tst_dialogs: the_language_is_chosen_in_settings_applied_at_once_and_remembered |
| 35 | `a_new_sources_thumbnails_are_made_as_it_is_scanned_before_any_grid_shows_them` | tst_scan: adding_a_source_reports_progress_thumbnails_and_reloads_the_grid |
| 36 | `adding_a_folder_from_the_catalogue_panel_scans_it_and_lists_it_without_copying` | tst_catalogue: adding_a_folder_scans_it_and_lists_it_without_copying |
| 37 | `the_dialog_says_why_a_folder_cannot_be_added` | tst_catalogue: same name |
| 38 | `a_folder_that_contains_a_source_asks_before_merging_it` | tst_catalogue: same name |
| 39 | `removing_a_source_confirms_with_the_numbers_and_can_be_undone_by_adding_it_again` | tst_catalogue: removing_a_source_confirms_with_the_numbers_and_adding_it_again_brings_the_photos_back |
| 40 | `dialogs_are_modal_no_other_window_opens_over_one_and_the_rest_waits` | tst_dialogs: a_click_behind_a_modal_dialog_does_nothing, the_guard_…, the_task_tabs_wait_for_the_dialog_that_is_open |
| 41 | `a_cards_banner_cannot_open_the_import_dialog_over_another_dialog` | tst_import: a_cards_banner_cannot_open_the_dialog_over_another_dialog |
| 42 | `the_question_about_removed_photos_waits_for_the_dialog_that_is_open` | tst_catalogue: same name (+ closing_the_question_with_escape_stops_the_scan) |
| 43 | `a_source_can_be_rescanned_to_pick_up_new_photos` | tst_catalogue: same name |
| 44 | `the_catalogue_panel_and_its_dialogs_render` | tst_views: catalogue, add-source, remove-source dialogs, both languages |
| 45 | `the_import_dialog_says_what_the_destination_is_to_the_catalogue` | tst_import: the_dialog_says_what_the_destination_is_to_the_catalogue |
| 46 | `a_plain_copy_from_the_dialog_registers_nothing_and_shows_no_photos_to_go_to` | tst_import: a_plain_copy_registers_nothing_and_shows_no_photos_to_go_to |
| 47 | `importing_into_a_new_folder_can_add_it_to_the_catalogue_in_the_same_gesture` | tst_import: same name |
| 48 | `a_card_with_camera_folders_offers_to_keep_them` | tst_import: same name |
| 49 | `the_import_forms_labels_get_the_room_their_translation_needs` | tst_import: the_labels_get_the_room_their_translation_needs |
| 50 | `the_folder_layout_choice_is_always_offered_and_dcim_itself_counts_as_a_camera_card` | tst_import: same name |
| 51 | `keeping_the_folders_of_a_chosen_dcim_folder_copies_the_camera_folders` | tst_import: same name |
| 52 | `a_card_inserted_while_the_application_runs_is_offered_for_import` | tst_import: same name |
| 53 | `a_card_that_was_already_in_when_the_workspace_opened_is_not_announced` | tst_import: same name |
| 54 | `the_import_dialog_and_the_card_banner_render` | tst_views: import dialog and card banner, both languages |

## What each platform shows

The suites run on Linux, Windows and macOS in CI on every push of the branch. CI keeps the pictures of
every view (`views-<platform>` artifacts, `qt/*.png`, English and French) for 14 days; they differ across
platforms and fonts by design (testing strategy §6) and are looked at, not compared.

## By hand (a person, on a real machine)

Not automated, and never driven by an automated session on a person's desktop: the system's own folder
dialogs (stay over the window, modal), typing with an input method (`é`, dead keys, Compose), the grey theme
and fonts on the real display, HiDPI, scrolling a large grid on the real GPU, the menu keys, the window
resize. Patrick's pass on Linux and Windows is Q0's checklist again, on the real application:
`cargo run -p auroraw-app` (see CONTRIBUTING.md).

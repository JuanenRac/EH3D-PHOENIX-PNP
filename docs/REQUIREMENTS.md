# EH3D-PHOENIX-PNP: requirements

This register is the single source of the state of each capability: `STATUS.md`, the README files and the capability matrix of the documentation hub cite it and never state a state that it does not. A state is one of `planned`, `prototype`, `implemented` and `validated`. `validated` means that a person has run the capability on a real machine and the evidence of the manifest says `bench` or `field`; nothing here is validated, because nothing of this program has moved a real machine.

The register has 161 requirements: 106 implemented, 33 prototype, 22 planned and 0 validated. By progress: 106 done, 33 partial, 16 pending and 6 blocked. High coverage is not proof of correctness, so each row says how it is checked: the test that names it, and what it shows.

## How to read the register

* **ID:** `PPNP-NNN`, never reused and never renumbered; a new capability takes the next number.
* **State:** `implemented` is code that is complete and tested on a computer, `prototype` is part of the capability, `planned` is not written.
* **Progress:** `done`, `partial`, `pending` (can be written without hardware) or `blocked` (waits for hardware or for another program of the suite).
* **Evidence:** `simulator` when the test runs against the simulated machine, camera or controller, `host-test` when it checks a calculation, a reader or a screen-less piece, `none` when nothing is written yet.
* **Test:** the acceptance criterion: `file::name` is the test `name` of `tests/file.rs`, which must exist. For a capability that is not written the cell says `planned:` and the name of the test that will show it; the day that test exists, the row must change.
* **Limits:** what is not done or not proven; a row that is `partial` or `blocked` always says it.

The check is `tests/reqs.rs`: it reads this table, finds each test and fails when a state, a test or a count does not agree.

## Machine model and settings

| ID | Requirement | State | Progress | Evidence | Test | Limits |
|---|---|---|---|---|---|---|
| PPNP-001 | The pieces of a machine (head, nozzle, camera, actuator, feeder, driver link) are separate and can be swapped for a simulated one. | prototype | partial | simulator | `heads::each_part_goes_to_a_head_that_reaches_it_and_is_given_back_in_board_coordinates` | The head, the camera model, the driver link and the actuators exist as separate pieces; there are no formal traits for all of them. |
| PPNP-002 | The machine is a plain settings file with a validator that refuses a wrong value with its line. | implemented | done | host-test | `model::a_bad_configuration_is_refused_with_its_code_and_its_line` | - |
| PPNP-003 | A length and a place carry their unit; a length converted and converted back is the same length. | implemented | done | host-test | `model::a_length_converted_and_converted_back_is_the_same_length` | - |
| PPNP-004 | The logical axes are told apart from the physical ones, with a transformation, soft limits and backlash. | prototype | partial | simulator | `backlash::the_settings_have_their_ranges_the_names_of_the_modes_and_the_example_is_the_default` | Backlash and the limits exist; there is no map from a logical axis to a physical one. |
| PPNP-005 | A part whose package or data is missing is told and does not stop the import. | prototype | partial | host-test | `feedlib::a_package_that_the_catalog_does_not_have_is_a_warning_only_when_there_is_a_catalog` | A part is not kept apart from the list of parts for later completion. |
| PPNP-006 | A part inherits what it does not say from its package, and the package from the defaults. | implemented | done | host-test | `model::a_part_inherits_from_its_package_and_the_package_from_the_defaults` | - |
| PPNP-007 | A part has a height, a depth through the board and the tips that may carry it, and they feed the safe height and the share of nozzles. | implemented | done | host-test | `model::the_defaults_of_a_part_come_from_the_configuration` | Retries and the bin of a part are not properties of the part. |
| PPNP-008 | A panel is a file of boards with places, turns and fiducials, and expands to the parts of every board. | implemented | done | host-test | `panel::the_boards_are_put_in_their_places_and_named_by_their_children` | - |
| PPNP-009 | The origin of a part and the rule of the bottom side are settings, and a part turned a half turn by another program is found. | implemented | done | host-test | `commands::check_bottom_shows_the_angle_each_rule_gives` | - |
| PPNP-010 | What a head carries (the tip, the part) survives a stop and a restart of a job. | prototype | partial | simulator | `vacuum::a_job_saved_and_resumed_ends_as_the_one_that_never_stopped` | The saved state keeps the step and the pick that stopped the job, not every nozzle. |
| PPNP-011 | The events of a job (a pick, a place, an error, the end) are told to whoever asked. | implemented | done | simulator | `hooks::a_job_has_the_events_of_what_happened_in_it` | - |

## Feeders

| ID | Requirement | State | Progress | Evidence | Test | Limits |
|---|---|---|---|---|---|---|
| PPNP-012 | With several feeders of the same part the one of higher priority is used first, and at a tie the nearest. | implemented | done | host-test | `feeding::the_feeder_of_higher_priority_is_used_up_first_and_then_the_other` | - |
| PPNP-013 | A feeder that fails too often in a window of picks is switched off and the job goes on with the others. | implemented | done | simulator | `vacuum::with_the_policy_defer_the_parts_that_never_pick_or_let_go_are_put_aside_with_their_code` | The window and the limit are settings; the failures are simulated. |
| PPNP-014 | A part can be given back to the reel without losing the count, and a feeder can be skipped once. | planned | pending | none | `planned:feeding::a_part_given_back_to_the_reel_keeps_the_count` | Only the count of a feeder that is refilled is handled today. |
| PPNP-015 | A tray is emptied cell by cell in the order it says, also turned. | implemented | done | host-test | `feeding::a_strip_and_a_tray_say_where_each_next_part_is` | - |
| PPNP-016 | A strip feeder gives the place of each next part by its pitch; the cover-sliding kind is not written. | prototype | partial | host-test | `feeding::a_turned_strip_and_a_turned_tray_turn_their_lines` | The strip is done; the blinds kind is not. |
| PPNP-017 | The families of feeders (tape, tray, strip, tube, bulk) are data with a pick place for each part. | prototype | partial | host-test | `feeding::a_tube_and_a_bulk_feeder_have_one_pick_place_whatever_is_taken` | Push-pull, drag, lever and vibratory feeders are not modelled. |
| PPNP-018 | A part lying loose among others is found by the camera and picked. | planned | blocked | none | `planned:vision::a_loose_part_is_found_among_others` | Needs a real camera. |
| PPNP-019 | A smart feeder has an address, a slot, an identify command and a version, over a bus that can be simulated. | prototype | partial | simulator | `nodelink::the_feeders_advance_run_out_jam_and_are_cleared_through_the_link` | The address, the frame and the bus belong to the feeder node; this program asks the node over HTTP for the state and the advance of a slot (1 to 32) and does not call identify. Tried against a node on a computer. |
| PPNP-020 | The feeders to load and the slots to put them in come from the board, nearest first, with a sheet for the person. | implemented | done | host-test | `slots::a_feeder_far_away_that_holds_most_of_the_parts_trades_places_with_one_that_stands_near` | - |
| PPNP-021 | What a feeder has left is counted, a short reel is told before the job, and the state after a job is written. | implemented | done | host-test | `deplete::a_strip_moves_its_next_place_along_and_the_other_kinds_keep_theirs` | - |
| PPNP-022 | The error of a pick seen by the camera is learned for the feeder and corrected. | prototype | partial | simulator | `learn::the_mean_of_the_readings_of_a_package_is_its_offset` | The offsets are learned by package from readings; there is no live return to the feeder. |

## Vision and calibration of the camera

| ID | Requirement | State | Progress | Evidence | Test | Limits |
|---|---|---|---|---|---|---|
| PPNP-023 | The vision is a chain of steps with settings, and a part is measured on a made-up picture. | prototype | partial | simulator | `vision::a_part_turned_30_degrees_in_64_by_64_pixels_is_measured_as_the_second_program_says` | A fixed chain of steps; it is not a list that a person edits. |
| PPNP-024 | A part on the nozzle is looked at and the place is moved and turned by what is seen. | implemented | done | simulator | `align::the_place_is_moved_and_turned_by_what_the_camera_sees` | With a simulated camera only. |
| PPNP-025 | The shots needed to see a part bigger than the camera are worked out. | planned | pending | none | `planned:vision::the_shots_for_a_part_bigger_than_the_camera_are_planned` | - |
| PPNP-026 | The measured fiducials are fitted to the designed ones, the residual of each mark is told and a fit that misses too much is refused. | implemented | done | host-test | `panel::two_marks_fix_a_turn_and_a_shift` | - |
| PPNP-027 | A fiducial next to a part refines the place of that part. | planned | pending | none | `planned:fidcheck::a_local_mark_refines_the_place_of_its_part` | - |
| PPNP-028 | The fiducials are located with the change of height taken into account. | planned | blocked | none | `planned:fidcheck::the_height_of_a_mark_changes_where_it_is_seen` | Needs a real camera. |
| PPNP-029 | The settings of the look are given for the machine, for a package and for a part, the most specific first. | prototype | partial | host-test | `recipe::the_values_of_the_rules_are_read_exactly` | Recipes give them by package, feeder and tip; there are no named profiles. |
| PPNP-030 | A placement is taught by placing a part once and measuring where it fell. | implemented | done | simulator | `learn::the_mean_of_the_readings_of_a_package_is_its_offset` | With readings from a file or a simulated camera. |
| PPNP-031 | The holes of a tape and the empty or full place of a nozzle changer are seen by the camera. | planned | blocked | none | `planned:vision::the_holes_of_a_tape_are_found` | Needs a real camera. |
| PPNP-032 | The camera waits until the picture has settled and the light stays on during a batch. | planned | pending | none | `planned:camera::the_picture_is_taken_after_the_wait_to_settle` | - |
| PPNP-033 | The outline of a part is drawn on the map with the mark of its direction. | implemented | done | host-test | `polarity::the_parts_with_a_direction_are_found_by_reference_or_by_package` | Drawn from the body in the table of packages. |
| PPNP-034 | A simulated camera gives pictures of parts and marks with noise on a fixed seed. | implemented | done | simulator | `camera::without_a_picture_the_camera_reads_the_part_through_its_lens` | - |
| PPNP-035 | The camera can be a system camera, a stream or a camera of another kind. | planned | blocked | none | `planned:camera::the_camera_is_chosen_by_its_name` | The live camera belongs to another program of the suite. |
| PPNP-036 | A code on a reel or a mark on a board is read by the camera. | planned | pending | none | `planned:vision::a_code_on_a_reel_is_read` | - |

## The job, its steps, its errors and its plan

| ID | Requirement | State | Progress | Evidence | Test | Limits |
|---|---|---|---|---|---|---|
| PPNP-037 | A job is a list of steps that can be paused, stepped, resumed and saved, and a job saved and resumed ends as the one that never stopped. | implemented | done | simulator | `job::a_saved_job_resumed_anywhere_ends_as_the_job_that_never_stopped` | - |
| PPNP-038 | Everything that would stop a job is checked before it starts, with a severity and a way to put it right. | implemented | done | host-test | `preflight::a_part_too_tall_or_too_deep_for_the_travel_of_z_is_an_error` | - |
| PPNP-039 | The order of the placements is a choice (nearest, by part, by height, by pick, by tip, as given) and every one is planned. | implemented | done | host-test | `compare::every_order_and_every_way_of_changing_the_tips_is_planned_as_the_configuration_would` | - |
| PPNP-040 | The way the nozzle tips are changed is a choice and the changes are counted. | implemented | done | host-test | `tips::the_tip_that_covers_the_most_parts_wins_and_the_preferred_one_breaks_a_tie` | - |
| PPNP-041 | A rank puts a hard order between blocks of parts and a preference inside a block. | implemented | done | host-test | `planner::a_hard_order_between_blocks_of_ranks_and_a_preference_inside_a_block` | - |
| PPNP-042 | The time of a move comes from the speed and acceleration of each axis, and the walk is improved by exchanges that never make it longer. | implemented | done | host-test | `planner::two_opt_never_makes_the_walk_longer_and_keeps_every_point` | - |
| PPNP-043 | A feed, a pick and a look can each be tried again as many times as the rules say. | implemented | done | simulator | `shop::retry_policy_gives_each_part_the_retries_of_the_first_rule_that_fits` | - |
| PPNP-044 | An error can stop the job at once or put the part aside to be looked at at the end. | implemented | done | simulator | `vacuum::with_the_policy_defer_the_parts_that_never_pick_or_let_go_are_put_aside_with_their_code` | - |
| PPNP-045 | The window shows which step an error stopped and why. | implemented | done | host-test | `jobtab::the_step_that_an_error_stopped_is_failed_and_the_row_says_why` | - |
| PPNP-046 | All the nozzles are turned together on long moves and the multi-nozzle order is planned. | prototype | partial | host-test | `pnp::more_nozzles_mean_less_travel` | The batches and the turns are planned; the turn before a long move is not a separate step. |
| PPNP-047 | The turn of a nozzle takes the shortest way that its range allows. | implemented | done | host-test | `model::a_limited_nozzle_turns_to_the_angle_in_its_range_that_is_nearest` | - |
| PPNP-048 | Hooks run a program at the events of a job; they are off by default and limited. | implemented | done | host-test | `hooks::hooks_are_off_by_default_and_their_settings_have_ranges` | - |
| PPNP-049 | A job ends with its statistics: time by step, by feeder and by nozzle, parts, failures. | implemented | done | simulator | `jobreport::the_tips_show_what_the_job_did_and_what_the_record_said_before` | - |
| PPNP-050 | Rejected parts are counted by reason and by feeder and can be sent to a bin chosen for each part. | prototype | partial | simulator | `shop::reject_report_ranks_the_reasons_and_fails_over_the_share_that_is_asked` | The reasons are counted; the bin of a part is not a setting. |
| PPNP-051 | A job stopped can go on from the parts already placed. | implemented | done | simulator | `job::a_saved_job_resumed_anywhere_ends_as_the_job_that_never_stopped` | - |
| PPNP-052 | The files opened last are listed and can be opened again. | prototype | partial | host-test | `docking::the_files_that_are_loaded_are_listed_as_recent_files_in_the_file_menu` | The list is kept by the frame of panels; the last job is not opened by itself. |

## Calibrations and the guided set-up

| ID | Requirement | State | Progress | Evidence | Test | Limits |
|---|---|---|---|---|---|---|
| PPNP-053 | The program detects what is wrong in the set-up, proposes the fix and can undo it. | implemented | done | host-test | `gui::the_problems_name_what_they_are_about_and_offer_a_fix` | - |
| PPNP-054 | The camera is calibrated from a pattern with the lens bending and a turned mount. | prototype | partial | simulator | `combined::the_fit_finds_the_camera_the_turn_of_the_sheet_and_the_bending_of_the_lens` | A made-up camera; the units per pixel by height are not modelled. |
| PPNP-055 | The runout of a nozzle is fitted from readings and applied. | implemented | done | simulator | `calibrate::the_runout_of_a_tip_is_found_exactly` | From readings; no camera under the nozzle. |
| PPNP-056 | A known move calibrates the cameras and the camera corrects the move. | prototype | partial | simulator | `combined::the_fit_finds_the_camera_the_turn_of_the_sheet_and_the_bending_of_the_lens` | The camera is calibrated from the moves of the head; the squareness of the axes is not. |
| PPNP-057 | The slack of the axes is a model that changes the time and the place of a move. | implemented | done | simulator | `backlash::simulate_says_what_the_slack_did_only_when_the_machine_has_some` | - |
| PPNP-058 | The machine homes against a mark on the table seen by the camera. | planned | blocked | none | `planned:calibrate::the_head_homes_against_a_mark_seen_by_the_camera` | Needs a real camera. |
| PPNP-059 | The heights of the nozzles are probed on the bed and a height that is off is told. | prototype | partial | simulator | `flow2::made_up_probings_are_the_same_for_the_same_plan_and_the_offsets_find_the_made_up_heights` | Made-up probings; no contact nozzle. |
| PPNP-060 | The thresholds of the vacuum are worked out from readings with a margin, and readings that overlap give none. | implemented | done | simulator | `vacuumcal::readings_that_overlap_give_no_threshold_and_say_so` | - |
| PPNP-061 | The head is brought to a mark by looking, moving and looking again, with a cap on every step. | implemented | done | simulator | `lookmove::no_step_is_longer_than_the_cap_whatever_the_error` | Against a made-up camera. |

## Motion and safety of the axes

| ID | Requirement | State | Progress | Evidence | Test | Limits |
|---|---|---|---|---|---|---|
| PPNP-062 | The time of a move follows a profile with a speed, an acceleration and a jerk for each axis. | prototype | partial | host-test | `planner::a_longer_move_never_takes_less_time_and_a_jerk_limit_never_makes_it_faster` | A model for the plan; nothing is sent to a motion planner. |
| PPNP-063 | A safe height is a setting and a part too tall for the travel of the axis is an error. | prototype | partial | host-test | `preflight::a_part_too_tall_or_too_deep_for_the_travel_of_z_is_an_error` | Free zones that save lifting are not modelled. |
| PPNP-064 | A pause is written as a dwell for the controller and not waited for in the host. | implemented | done | simulator | `driver::a_placement_goes_up_over_the_feeder_down_pump_on_up_over_the_place_down_pump_off_blow_up` | - |
| PPNP-065 | The feed and the acceleration are sent only when they change. | planned | pending | none | `planned:driver::a_feed_that_did_not_change_is_not_sent_again` | - |
| PPNP-066 | An axis does not move while a signal (a door, the nozzles lifted, a valve) is not as a rule of the profile requires, and an unknown signal refuses the move. | implemented | done | simulator | `interlock::a_move_in_x_and_y_is_refused_while_the_blow_valve_is_on_and_the_session_stops` | Signals are the actuators and the sensors that a program reports; no sensor of a real machine has been read. |
| PPNP-067 | A park place, a discard place and the places of a nozzle changer are named in the settings. | planned | pending | none | `planned:driver::the_park_place_is_where_the_head_rests` | - |
| PPNP-068 | A nozzle changer takes tips in and out in an order, with a manual change that stops the job once. | prototype | partial | simulator | `sim::a_pick_and_a_place_take_their_time_and_a_tip_change_takes_its_own` | Tip changes are planned and timed; the sequence of a changer is not. |

## The window and the way a person works

| ID | Requirement | State | Progress | Evidence | Test | Limits |
|---|---|---|---|---|---|---|
| PPNP-069 | The work table is drawn with the board, the parts, the feeders, the polarity, the path of the head and the next part. | implemented | done | host-test | `gui::the_scene_holds_the_parts_the_feeders_and_the_path` | The drawing has not been reviewed on a screen. |
| PPNP-070 | The tables share a selection, sort, filter and remember the width of their columns. | implemented | done | host-test | `gui::the_table_of_parts_shows_the_feeder_and_sorts` | - |
| PPNP-071 | The window is made of panels that dock, float, fold, move and are saved in named arrangements. | implemented | done | host-test | `docking::the_window_has_the_seven_panels_of_the_sector_and_those_that_the_tabs_had` | The drawing has been run only in a context with no window. |
| PPNP-072 | The log has levels and a diagnostic that can be copied and has no secret in it. | implemented | done | host-test | `gui::the_log_keeps_the_last_lines_and_says_them_as_text` | - |
| PPNP-073 | The head is jogged with the keys, in steps, and not while a text box has the keyboard. | planned | pending | none | `planned:gui::the_jog_keys_move_the_head_in_steps` | Needs a machine to jog. |
| PPNP-074 | The colours of the window are a choice between three themes. | implemented | done | host-test | `gui::the_themes_have_keys_and_distinct_colours` | - |
| PPNP-075 | A window that was left outside the screen is brought back at the start. | planned | pending | none | `planned:gui::a_window_outside_the_screen_is_brought_back` | - |
| PPNP-076 | A guide of five steps takes a person from the files to a simulated job and goes forward only when the step is done. | implemented | done | host-test | `gui::the_guide_goes_forward_only_when_the_step_is_done` | - |
| PPNP-077 | The state of the machine is sent to the EH3D server through the common client. | implemented | done | simulator | `server::the_address_the_machine_and_the_token_are_checked_before_anything_is_sent` | Tried against a server in memory. |
| PPNP-078 | Every word of the window exists in the eight languages and is really translated. | implemented | done | host-test | `gui::every_word_has_a_row_and_every_row_has_eight_filled_columns` | - |
| PPNP-079 | Every setting has a help in the language of the person. | prototype | partial | host-test | `toolpanels::the_new_words_are_in_the_eight_languages_and_in_the_language_of_portugal` | The tools and the commands have a hint; the settings have comments in the example file. |
| PPNP-080 | Samples go from a small board to a large one and all plan. | implemented | done | host-test | `gui::the_samples_go_from_small_to_large_and_all_plan` | - |

## Files in and out

| ID | Requirement | State | Progress | Evidence | Test | Limits |
|---|---|---|---|---|---|---|
| PPNP-081 | A table of positions is read with the usual names of the columns, its unit from the header and its sides by their first letter. | implemented | done | host-test | `import::the_unit_comes_from_the_header_the_cell_the_map_or_the_settings` | - |
| PPNP-082 | A file of another program (Altium) is read with its places in mils and its layers in words. | implemented | done | host-test | `flow2::an_altium_file_with_places_in_mils_and_layers_in_words_is_read` | Not compared with a file written by the program itself. |
| PPNP-083 | A footprint of KiCad gives the body of a package. | implemented | done | host-test | `kicadmod::the_row_has_the_body_the_tape_and_the_height_of_the_package_or_the_person` | The pads are not kept. |
| PPNP-084 | The mounting text of EAGLE is read with its unit and its turn. | implemented | done | host-test | `import::eagle_declares_its_unit_in_a_comment_and_accepts_a_plain_angle_when_the_format_is_given` | - |
| PPNP-085 | A table may be UTF-8 or UTF-16 with a byte order mark. | prototype | partial | host-test | `encoding::a_table_of_parts_in_utf16_gives_the_parts_of_the_one_in_utf8` | Other tools are read by the map of the columns, not by a profile of their own. |
| PPNP-086 | The paste is taken from a Gerber file of the stencil. | planned | pending | none | `planned:dfm::the_apertures_are_read_from_a_gerber_file` | The apertures come from a table of features. |
| PPNP-087 | A turn that another program puts on a package is corrected by a table of rules. | implemented | done | host-test | `model::the_rotation_rules_keep_the_order_of_the_file_and_the_headers_extend_the_reader` | - |
| PPNP-088 | A table of packages is made from their standard names. | implemented | done | host-test | `pkggen::the_small_outlines_the_soic_and_the_tssop_have_their_bodies` | - |
| PPNP-089 | A plan is written as JSON, a load sheet, a report, a placement list for assemblers and the files that an assembly house asks for. | implemented | done | host-test | `exchange::a_plan_is_written_with_every_part_pending_and_read_back` | - |
| PPNP-090 | The spaces around the identifier of a part are never part of it. | implemented | done | host-test | `model::spaces_around_the_names_are_never_part_of_them` | - |

## Drivers and actuators

| ID | Requirement | State | Progress | Evidence | Test | Limits |
|---|---|---|---|---|---|---|
| PPNP-091 | A G-code driver is a profile file with the lines, the patterns, the axes and the pins, tried against a simulated controller. | implemented | done | simulator | `driver::a_move_is_a_line_and_the_line_that_waits_for_it` | Marlin and Klipper only, and never against a real controller. |
| PPNP-092 | The link to the controller is a piece that can be a simulated serial line or a simulated Moonraker. | prototype | partial | simulator | `driver::through_moonraker_the_patterns_must_read_what_the_link_writes` | No port and no network connection is opened. |
| PPNP-093 | An actuator is a name with a line that switches it on and one that switches it off, and the state is known to the session. | prototype | partial | simulator | `vacuum::the_actuators_are_listed_one_to_a_line_with_a_name_and_a_delay` | On and off only; no PWM, no reading and no web actuator. |
| PPNP-094 | A tower shows green, yellow or red from the journal, and a sound can be asked for. | prototype | partial | simulator | `simviews::the_tower_is_red_on_a_failure_and_yellow_on_a_slow_step_and_on_a_long_wait` | The logic is done; no real tower is driven. |
| PPNP-095 | A commercial machine is described with data. | planned | blocked | none | `planned:driver::a_commercial_machine_is_a_profile` | Needs the machine. |
| PPNP-096 | A simulated machine has a virtual clock, delays and faults that can be forced on a seed. | implemented | done | simulator | `driver::the_faults_do_what_they_say` | - |

## Safety and robustness

| ID | Requirement | State | Progress | Evidence | Test | Limits |
|---|---|---|---|---|---|---|
| PPNP-097 | A setting that would be dangerous is refused by the validator, and all limits are checked together. | implemented | done | host-test | `model::the_safety_limits_are_checked_together` | - |
| PPNP-098 | A nozzle never picks while it holds a part and never places while it holds none. | implemented | done | simulator | `sim::a_nozzle_never_picks_while_it_holds_a_part_and_never_places_while_it_holds_none` | - |
| PPNP-099 | A stuck part stops the job holding it and the person decides. | implemented | done | simulator | `vacuum::a_stuck_part_stops_the_job_holding_it_and_the_place_is_tried_again_by_the_person` | - |
| PPNP-100 | A damaged file never panics: every example is cut and damaged in many ways. | implemented | done | host-test | `fuzz::a_board_that_is_written_is_read_back_whatever_the_shape_and_the_unit` | - |
| PPNP-101 | A file larger than the limit is refused with its size. | implemented | done | host-test | `commands::a_file_larger_than_the_limit_of_the_configuration_is_refused` | The limit is a setting of the machine file. |
| PPNP-102 | A program that a hook runs is off by default, in a list of allowed names and limited in time and output. | implemented | done | host-test | `hooks::a_program_that_takes_too_long_is_stopped` | - |
| PPNP-103 | A diagnostic can be copied without a token or a password in it. | prototype | partial | host-test | `serverui::the_lines_that_are_kept_never_have_the_token` | The token is kept out; paths are not made anonymous. |

## Tests and quality

| ID | Requirement | State | Progress | Evidence | Test | Limits |
|---|---|---|---|---|---|---|
| PPNP-104 | A job is tried from the files to the end on the simulated machine, for a board and for a panel. | implemented | done | simulator | `gui::the_state_goes_from_files_to_a_finished_job_without_a_window` | - |
| PPNP-105 | The retries and the failures of a feeder are tried with faults that are forced. | implemented | done | simulator | `vacuum::a_pick_that_leaves_no_part_on_the_nozzle_is_tried_again` | - |
| PPNP-106 | The geometry and the units are tried back and forth. | implemented | done | host-test | `model::turning_a_place_forth_and_back_gives_the_same_place_and_keeps_its_distance_to_the_origin` | - |
| PPNP-107 | The vision is tried on pictures made by the program, with noise on a fixed seed. | implemented | done | simulator | `vision::a_noise_that_crosses_the_threshold_is_taken_out_by_the_filter_and_otsu` | - |
| PPNP-108 | The driver is tried against a simulated controller with the answers that confuse a pattern. | implemented | done | simulator | `driver::the_cases_of_the_firmware_that_confuse_a_pattern_are_each_read_right` | The answers of the examples are written by hand, not captured. |
| PPNP-109 | No two files of the repository have names that differ only in case or that Windows cannot hold. | implemented | done | host-test | `names::no_two_files_have_names_that_differ_only_in_case` | - |
| PPNP-110 | The documentation of the library is built by the CI. | planned | pending | none | `planned:readme::the_documentation_of_the_library_is_built` | - |
| PPNP-111 | A test checks that every word has its row in the eight languages. | implemented | done | host-test | `gui::every_word_has_a_row_and_every_row_has_eight_filled_columns` | - |
| PPNP-112 | The files of the examples are the program's own, and a file of another program says its source. | prototype | partial | host-test | `kicadmod::the_row_has_the_body_the_tape_and_the_height_of_the_package_or_the_person` | The examples are written for this program; the licence of a file of another program is not recorded. |

## Packages and footprints

| ID | Requirement | State | Progress | Evidence | Test | Limits |
|---|---|---|---|---|---|---|
| PPNP-113 | A pad has a shape for the picture of a part. | planned | pending | none | `planned:kicadmod::the_pads_of_a_footprint_are_kept_with_their_shape` | The body of a package is kept; its pads are not. |
| PPNP-114 | A package is made from its name by the formulas of the standard. | implemented | done | host-test | `pkggen::the_small_outlines_the_soic_and_the_tssop_have_their_bodies` | - |
| PPNP-115 | A part whose package changes is checked against the table of packages. | prototype | partial | host-test | `gui::a_package_table_that_was_edited_survives_the_loading_of_the_table_of_parts` | Checked when the tables are loaded; not when one is edited. |
| PPNP-116 | A package can be copied and pasted as text. | planned | pending | none | `planned:gui::a_package_is_copied_as_text` | - |

## Knowledge of the machine and of the work

| ID | Requirement | State | Progress | Evidence | Test | Limits |
|---|---|---|---|---|---|---|
| PPNP-117 | A guide compares a machine for placing with a printer and a mill, in words of its own. | planned | pending | none | `planned:readme::the_design_guide_is_linked_from_the_readmes` | Belongs to the documentation of the suite. |
| PPNP-118 | A checklist of the mechanical causes of a bad accuracy is part of the set-up. | planned | pending | none | `planned:gui::the_set_up_lists_the_mechanical_checks` | - |
| PPNP-119 | A diagnostic repeats the checks of a job that goes wrong: the origin, the turn of one part, a feeder. | prototype | partial | host-test | `preflight::a_part_too_tall_or_too_deep_for_the_travel_of_z_is_an_error` | The checks before a job exist; the diagnostic that repeats them step by step does not. |
| PPNP-120 | A training plan and tutorials in the eight languages. | planned | pending | none | `planned:readme::the_tutorials_are_listed_in_the_readmes` | - |

## Beyond the first list

| ID | Requirement | State | Progress | Evidence | Test | Limits |
|---|---|---|---|---|---|---|
| PPNP-121 | A process is a list of phases (load, place a side, oven, solder, inspect); a phase starts only after the earlier ones, and its load-out is fixed once it begins. | implemented | done | simulator | `process::a_phase_starts_only_after_the_earlier_ones_and_when_none_is_going` | Phases are told to a person; the states are kept in a file, not in the EH3D server. |
| PPNP-122 | The guided manual mode walks a process phase by phase: a side placed by hand one part at a time, the other tasks until the person says done. | implemented | done | host-test | `process::the_manual_process_walks_the_phases_in_order_and_places_each_side_by_hand` | Nothing is moved: the person does the work. |
| PPNP-123 | Eleven commands of the driver and the assistants run from a panel of the window with the arguments typed, bounded, without a shell and with a time limit. | implemented | done | host-test | `cmdpanel::the_other_commands_run_through_a_panel_the_same_way` | The panels show what the command printed; they do not move anything but a simulated controller. |
| PPNP-124 | Each requirement of this register names a test that exists, and a state that agrees with it. | implemented | done | host-test | `reqs::every_requirement_names_a_test_that_exists` | - |

## Slots, nodes, panels and tables

| ID | Requirement | State | Progress | Evidence | Test | Limits |
|---|---|---|---|---|---|---|
| PPNP-125 | A tape feeder takes as many slots as its tape and the gap ask for; no two feeders share a slot; each lies inside the bank and inside the reach of the head. | implemented | done | host-test | `slotbank::a_bank_of_sixty_slots_holds_twenty_tapes_of_eight_millimetres` | The sizes of a slot are settings; none has been measured on a machine. |
| PPNP-126 | A slot is free, occupied, depleted or jammed, and the moves between the states are one table that a test walks entire. | implemented | done | host-test | `slotbank::the_table_of_the_moves_between_the_states_is_walked_completely` | - |
| PPNP-127 | The state of a feeder in the chain of a feeder node moves the state of its slot, and a feeder that does not answer moves nothing. | implemented | done | simulator | `slotbank::the_chain_of_a_feeder_node_drives_the_slots_of_the_banks` | Tried against a feeder node on a computer; no real node has answered. |
| PPNP-128 | The head node and the feeder node are asked over HTTP through the common client, and a refused token, silence and a refusal are told apart. | prototype | partial | simulator | `nodelink::a_refused_token_is_told_and_the_state_is_still_read_without_it` | The reason of a refusal is the `why` of the node (see the next rows and `COMMON_REQUESTS.md`); nothing has talked to a real node. |
| PPNP-129 | A node that cannot be heard for `node_silence_limit` questions, or that says it is safe, is held; nothing that moves is asked of it until a person acknowledges. | implemented | done | simulator | `nodelink::a_head_that_cannot_be_heard_is_never_taken_to_be_clear_and_is_held_until_it_answers_and_a_person_acknowledges` | The hold is the program's; the safe state of a real node is the node's. |
| PPNP-130 | The machine moves in X and Y only while the head has just said every nozzle is raised; a head that is silent or refuses the token is never taken to be clear. | implemented | done | simulator | `nodelink::the_axes_of_the_machine_move_only_while_the_head_says_it_is_clear` | The answer of a real head has not been read. |
| PPNP-131 | The faults of a node on a computer (silence, a refused token, a refusal) come from a fixed seed, so a run is the same every time. | implemented | done | simulator | `nodelink::the_faults_drawn_from_a_seed_come_the_same_every_time_and_a_run_ends_in_a_sound_state` | - |
| PPNP-132 | The window has a panel of the slots of the banks and a panel of the nodes, hidden at the start and opened from the menus, with the nodes shown as made up. | prototype | partial | host-test | `nodedock::the_panels_of_the_slots_and_of_the_nodes_are_in_the_frame_hidden_and_named_in_eight_languages` | The drawing has been compiled but no person has looked at it on a screen. |
| PPNP-133 | An arrangement of the panels saved before the new panels existed still loads, with the new panels hidden and the rest where it was. | implemented | done | host-test | `nodedock::an_arrangement_saved_before_the_panels_existed_loads_with_the_new_ones_hidden_and_keeps_its_shape` | The frame notes that it put the new panels among the hidden ones; the window does not show that note. |
| PPNP-134 | A panel is composed from the size of the sheet, its four rails, the outline of each board and the units put and turned, each of any board. | implemented | done | host-test | `panelcomp::the_example_is_sound_and_the_boxes_of_its_units_are_where_the_turn_puts_them` | An outline is a rectangle; the offset of the board file and the tooling holes are not modelled. |
| PPNP-135 | A unit outside the rails, two units that overlap and two closer than the least distance are found, and a composition that does not fit writes no panel file. | implemented | done | host-test | `panelcomp::the_check_agrees_with_a_count_of_points_on_pairs_of_units_chosen_by_a_fixed_seed` | - |
| PPNP-136 | The panel file of a composition is the same whatever the order of the units, and the reader of panels takes it back. | implemented | done | host-test | `panelcomp::the_panel_file_is_the_same_whatever_the_order_of_the_units_and_the_numbers_are_written_one_way` | - |
| PPNP-137 | Every example that the help teaches is run by a test and must end well. | implemented | done | host-test | `helpdemo::every_example_of_the_help_runs_and_prints_something` | The examples of the help are a few; the README commands have their own test. |
| PPNP-138 | A check that is not passed ends the command with the code 2, in every command that verifies something, and the help says the three codes: 0 when all is well, 1 when the command could not be run, 2 when a check is not passed. | implemented | done | host-test | `exitcodes::check_plan_and_preflight_end_with_2_when_the_feeders_do_not_cover_the_board` | A write refused because the file changed since it was read ends with 1: the command did not run. |
| PPNP-139 | A value that looks like a number (a part number, a package, an identifier with leading zeros) is kept as text by the readers of tables (parts, feeders, bills of materials, slots, banks, the catalog, prices and lots). | implemented | done | host-test | `textkeep::the_catalog_the_prices_and_the_lots_keep_what_was_written` | - |
| PPNP-140 | A write to a shared table is refused when the file changed since it was read: the table of feeders, of the banks, of parts made by an import and the queue, from the commands, take `--revision`. | implemented | done | host-test | `revision::the_table_of_the_banks_is_edited_in_place_only_while_it_has_the_revision_that_was_read` | The other commands write new files and never over one. The check and the write are two steps, not one lock. |
| PPNP-141 | The hooks are told while the job goes: before and after every pick and place, and when a part is complete, each run with the limits of the hooks (off by default, a list of programs, no shell, bounded arguments, a time limit, a limit of runs for the whole job). | implemented | done | simulator | `hooks::simulate_tells_the_hooks_while_the_job_goes_with_a_fixed_seed` | The events of the camera (before and after a capture) wait for a live camera. |
| PPNP-142 | The data of an event also go to the program as one line of JSON of at most 1024 bytes on its input, and a program that does not read it does not hold the job up. | implemented | done | host-test | `hooks::the_data_of_an_event_go_to_a_program_as_one_bounded_line_of_json` | - |
| PPNP-143 | What a nozzle is doing (idle, picking, carrying, placing) is told in every row of the journal and in the events of the hooks. | prototype | partial | simulator | `journal::every_row_says_what_the_nozzle_is_doing_after_the_step` | The view of the job and the signal tower do not show it yet; the journal says the state after a step, not the time spent in each. |
| PPNP-144 | A path is kept in a saved file with `/` between the names, without the prefix of Windows and without `..`, and two spellings of one file are told to be the same. | implemented | done | host-test | `pathid::two_spellings_of_one_file_have_one_identity_and_capitals_follow_the_system` | Only the queue and the check that an output is not an input use it today; it has been tried on Windows and Linux in the tests, not on a stick that moves between them. |
| PPNP-145 | The places can be ordered by a list of keys (unit, feeder, part, height, area, quantity), each ascending or descending, in `placement_order`. | implemented | done | host-test | `placeorder::the_plan_follows_the_list_unit_by_unit_and_a_list_of_nothing_changes_nothing` | The cost of a part is not a key: the prices are in a table the planner does not read. |
| PPNP-146 | A wiring sheet (what goes to which pin, and whether the stop lets each one go) and a drawing of the pneumatics are made from a driver profile. | implemented | done | host-test | `wiring::a_marlin_profile_gives_the_axes_the_pump_and_the_valve_with_what_a_stop_lets_go` | The pins are the ones the profile names; whether they are the pins of a given board is for the person who wires it to check. |
| PPNP-147 | The window reads a table of the banks from a file, edits the state of a feeder in place and saves it back only while the file is as it was read. | prototype | partial | host-test | `nodedock::the_bench_reads_a_table_from_a_file_edits_it_in_place_and_saves_it_back_with_its_revision` | The buttons are drawn but no person has looked at them on a screen. |
| PPNP-148 | A token that a node refused is kept by the link, so that the next question asks the node again and a node that refuses it once is not taken to refuse it for ever. | implemented | done | simulator | `nodelink::a_node_that_refuses_the_token_is_asked_again_with_it_and_a_link_that_is_fixed_is_never_logged_out` | Tried against nodes on a computer; the reason of a refusal is in the last row. |
| PPNP-149 | A driver profile says how the machine homes: with end switches, or without them with a sensitivity for X and one for Y that go, with a line of the profile, before the lines of `home`; the sensitivities without the mode, and the mode without them, are refused. | implemented | done | simulator | `homingmode::the_sensitivities_are_the_first_lines_of_the_homing` | - |
| PPNP-150 | Two dumps of the configuration of a firmware (the answer to `M503`) are compared value by value, `80` and `80.00` are the same, and a change of firmware warns to reset the EEPROM. | implemented | done | host-test | `fwdiff::the_changes_are_the_ones_of_the_two_examples` | - |
| PPNP-151 | The sheet of the machine (table, machine, work area, smallest part tried, greatest speed) comes from the settings, what nobody measured says "not measured" in eight languages and no figure is made up, and a report of a job carries it when a field is given. | implemented | done | host-test | `machsheet::the_report_of_a_job_carries_the_sheet_only_when_a_field_is_given` | - |
| PPNP-152 | What each nozzle is doing is shown in the Job tab, and the signal tower names the nozzle and its state next to every light that is not green. | implemented | done | simulator | `nozzleview::a_light_that_is_not_green_names_the_nozzle_and_what_it_was_doing` | The Job tab draws the line of the nozzles; no person has looked at it on a screen. |
| PPNP-153 | The link to a node gives the `why` of a refusal as the node wrote it, keeps a fixed token after a `401` and can keep a masked piece of the body of a refusal. | implemented | done | simulator | `nodereason::a_409_gives_the_why_the_node_wrote_and_nothing_else` | Built and tried against the common client at release 0.3.0, which the CI is pinned to; no real node has answered. |
| PPNP-154 | The lines that a driver profile writes for a whole job have a SHA-256 fingerprint (each line counted with its line break): the same plan and profile always give the same one, and a place that moves by a thousandth of a millimetre gives another. | implemented | done | simulator | `toolpath::a_place_that_moves_by_a_thousandth_of_a_millimetre_gives_another_fingerprint` | It is the fingerprint of the lines of this program, not the one the server makes of its request; nothing is sent to a controller. |
| PPNP-155 | `toolpath-hash --expect HASH` ends with the code 2 when the lines are not the ones of that hash, so a plan that changed after a person saw it is not run as the authorised one; a text that is not a fingerprint is a mistake of the command (code 1). | implemented | done | simulator | `toolpath::expect_ends_with_2_when_the_lines_are_not_the_authorised_ones` | The authorisation itself is made on the server; this only tells whether the lines are still the ones that were seen. |
| PPNP-156 | The configuration of a Klipper controller (sections, `key: value` lines, macros, the values saved by `SAVE_CONFIG` winning over the file above them) is compared value by value, and a Klipper file is never compared with a Marlin dump. | implemented | done | host-test | `klippercfg::the_changes_are_the_ones_of_the_two_examples` | The files of the examples are written by hand; no file saved by a real controller was read. |
| PPNP-157 | The height of a bent board is worked out from a whole grid of measured points by bilinear interpolation (a plane is followed exactly), the change of height of each placement is given against a zero, and a place outside the grid has no correction. | implemented | done | host-test | `heightmap::a_flat_tilted_board_is_followed_exactly_from_a_fixed_seed` | The grid is typed or made by a person; no sensor of a machine measured one. The correction is not yet applied to the height of a placement in a job. |
| PPNP-158 | A part that no feeder holds goes to the feeder that holds an equivalent part (a table of groups says which) and has the most parts left after the parts that need it for themselves, and the answer does not depend on the order of the feeders. | implemented | done | host-test | `equivalent::the_answer_does_not_depend_on_the_order_of_the_feeders` | The groups are given by the person: the program does not know which parts are equivalent. |
| PPNP-159 | A drag feeder moves the tape by a whole number of sprocket holes and of pockets: the stroke, the picks that it serves and the strokes of a tape are worked out, and a pitch that needs more picks than the setting allows is refused. | implemented | done | simulator | `drag::pitches_from_a_fixed_seed_keep_the_stroke_whole_in_holes_and_in_pockets` | The pick places inside a stroke are not yet read by the planner, and no drag feeder was looked at with a camera (`tape-locate` belongs to the vision program). |
| PPNP-160 | An entry of the queue goes up, down, to the first or to the last place it may take; what is running or made keeps its place, and the order of what was made never changes. | implemented | done | host-test | `queuemove::moves_from_a_fixed_seed_never_lose_an_entry_or_move_a_fixed_one` | - |
| PPNP-161 | The window has a panel for each of fifteen commands (the four new ones are `toolpath-hash`, `height-map`, `equivalents` and `drag-plan`) and two buttons of the queue to move an entry. | prototype | partial | host-test | `cmdpanel::the_fifteen_commands_have_a_name_a_panel_a_title_and_a_hint_each` | The drawing is not covered by tests and no person has looked at it on a screen. |

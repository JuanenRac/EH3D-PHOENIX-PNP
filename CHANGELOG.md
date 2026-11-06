# Changelog

All notable changes to this project are documented here.

## [Unreleased]

- **A picture mode for the window:** `--shot FILE.png` (built with `--features gui`, with the command line as `phoenix-pnp --shot FILE.png FILES...` or with `phoenix-pnp-gui`) opens the window at the size of `--size WxH` (320 to 8192, 1180x760 by default) with the files given (bare names or `--open FILE`) loaded, in the theme of `--theme phoenix|light|contrast` and the language of `--lang CODE` (English by default), waits `--wait N` frames (0 to 3600, 6 by default), saves what it drew as a PNG file and closes by itself. An existing file is never replaced unless `--replace` is given; in this mode the window reads and writes no settings (not even the file of the window or `last_error.txt`), the guide is not shown, the exit code is 0 when the file was written, 1 when it was not (also when the picture does not come back within 600 frames) and 2 for wrong arguments, and a build without the window says so. The arguments are read by plain code with its tests.
- **Parts that are not to be mounted (do not populate):** a column of a CSV position file called `dnp`, `do not place`, `do not populate`, `do not mount`, `do_not_place`, `not fitted` or `no montar` leaves the part out with a yes (`yes`, `y`, `true`, `1`, `x`), and a column called `fitted`, `populate`, `populated`, `mount`, `mounted` or `montar` leaves it out with a no (`no`, `n`, `false`, `0`, `-`); an empty cell mounts the part. The parts left out are not in the plan, the load sheet or the checks, each one is listed as an info finding (`info: part R2: marked as not to be mounted`), and the text report ends with their references. `--dnp R1,R2` does the same for a file with no such column and refuses a reference that no part has. The window keeps them apart too, and a panel lists those of its boards once. Library: `parts::parse_all` gives `Parsed { parts, dnp }`, and `take_references` takes references out of a list of parts.
- **The words and the names of the columns of mounting are settings:** `header_dnp` and `header_fitted` add names for the column, `dnp_yes_words` and `dnp_no_words` add words for yes and no; `examples/machine.cfg` documents the four keys.
- **`phoenix-pnp tables-check`:** reads each table that is named (`--parts`, `--packages`, `--catalog`, `--feeders`, `--slots`, with `--config` and `--lang`), prints `ok:` with the number of rows or `not valid:` with the code and the row of the error in the language asked for, reads all of them before it stops, and exits with 1 when any is not valid.
- **Examples:** `examples/board_dnp.csv` (a board with a `dnp` column), `examples/feeders_kinds.csv` (a tape, a strip, a tray filled in a snake and a feeder that is switched off) and `examples/packages_common.csv` (a table of common packages to adjust to your own parts), all read by `tables-check` in a test.
- **`simulate --report-csv FILE`:** the statistics of the job as a CSV with the header `section,name,value,picked,pick_failures,placed,place_failures,faults,disabled` that never changes: the totals, a row for each package and each feeder, and a row for each part put aside with its reason; it is never written over a file the job was made from.
- **New error codes:** `PHOENIX-PNP-033` (a mark of mounting that is neither yes nor no, or a reference to leave out that is not known) and `PHOENIX-PNP-034` (the file of positions is not valid: every error of the position file now carries it, with the row or the line), with their meaning in the eight languages and in the eight READMEs.
- **The align step in the job:** with `align = on`, after a part is picked the camera looks at it and says how far it is from the nozzle (X and Y) and how much it is turned; the place goes to the planned place less that offset and the nozzle turns less that turn. A part that the camera finds further off than `pick_tolerance_mm` is thrown away (it is gone from its feeder, and the next part is fed) and picked again as `pick_retries` and `error_policy` (`alert` or `defer`) say. Off by default: with the step off a job (its plan, its events, its reports and its saved state) is exactly what it was.
- **Settings of the step:** `align` (`off` or `on`), `pick_tolerance_mm` (0.001 to 50), `align_passes` (1 to 5 looks, each one correcting what the last left) and `align_settle_s` (0 to 10 seconds to wait before every look), documented in `examples/machine.cfg`.
- **The time of the step is in the plan:** every look adds `align_settle_s` to the time of the plan (the text report says how much of it is the camera, and the JSON plan has `align_s` only when the step is on) and to the clock of the simulated machine.
- **The simulated camera:** `sim_align_dx_mm`, `sim_align_dy_mm` and `sim_align_deg` (how far every part is from the nozzle), `sim_align_noise_mm` (a random error of each reading, drawn from `sim_seed`) and `sim_camera_fail_rate`; a look can be made to fail by chance or at a chosen look (`Job::camera_mut`). The camera has a generator of its own, so the failures of the picks and the places do not move when it is used.
- **Statistics of the correction:** the parts corrected, the mean and the largest distance, the largest turn, the parts thrown away and the looks without a reading, in the text of `simulate`, in the JSON report (`align`) and in a new section `align` of `--report-csv` that follows the rows that were always there (the header does not change); a saved job keeps them and the corrections of the parts on the nozzles.
- **New error codes:** `PHOENIX-PNP-035` (the camera finds the part further off than the tolerance) and `PHOENIX-PNP-036` (the camera gives no reading of the part), with their meaning in the eight languages and in the eight READMEs.

## [0.0.5] - Fiducials of panels, tip order search, editable tables, failure statistics and the window without a console

- **The validation job is the shared one of the documentation hub:** the checks of the manifest, the documents and the pictures run through the reusable workflow, fixed to a full commit of EH3D-DOCS whose actions are pinned by commit.
- **Panels and boards tabs, with the fiducials of the panel:** the measured places of the marks are fitted to the designed ones (a similarity with two marks, an affine fit with three or more, as `fiducial_mode` says), the parts of the panel are moved and turned by the fit, every mark shows its residual, and a fit that misses a mark by more than `fiducial_max_residual_mm` is refused; the same from the command line with `--fiducials FILE` (a CSV `id,x,y`) next to `--panel`. New error code `PHOENIX-PNP-032`.
- **The widths of the columns are remembered** between runs, in the settings file of the user (drag the edge of a column head).
- **A file picker inside the window,** also to choose the output folder and the file of measured marks, besides typing the path.
- **The tables of feeders and packages can be edited in the window** (value, package, place, count, warning level, slot, priority, kind, the cells of a tray, the pitch of a strip, and the properties of a package), every edit can be taken back (`undo_limit` steps), and the tables that were edited are written with the outputs.
- **Trays are emptied by rows, by columns or in a snake** (the `fill` column of the feeders, `tray_fill` for those that say nothing).
- **`tip_strategy = optimize`:** the parts are brought together by nozzle tip and the order of the tips is searched for the shortest way and time, counting the trip to the tip changer and the tip already on the nozzle when a block of ranks starts; the search stops at `tip_opt_max_evals` orders or `tip_opt_time_limit_s` seconds.
- **`phoenix-pnp check-bottom`** shows, for the parts of the bottom side of a position file, the angle that each rule (`keep`, `mirror`, `half_turn`) gives and the one in use, to compare with a real file.
- **Failures by package and by feeder in the simulator** (`sim_part_fail`, `sim_feeder_fail`), statistics by package and by feeder, and the parts per hour of the simulated job (in `simulate`, its report and the log of the window).
- **`phoenix-pnp-gui`,** the window as a program of its own that opens without a console on Windows; `phoenix-pnp` keeps its console (the reasons are in `docs/windows-console.md`).
- **The Windows executable carries the icon of the program:** the file shows it in the explorer and the taskbar, together with the name, the description and the version, embedded by `build.rs`; other systems are not affected.
- **Error codes:** every error of the new code has a code `PHOENIX-PNP-NNN` with its short meaning in the eight languages, and the eight READMEs list them (a test checks it).
- **Typed lengths and places:** built once from millimetres, mils or inches, so units cannot be mixed.
- **Machine configuration file:** `key = value` lines with a schema version, a validator that names the line, safety limits, and `examples/machine.cfg` documenting every key; the options of the command line win over it.
- **Tables of parts and packages:** height, depth through the board, pick retries, bin, nozzle tip, rank, origin correction and tape width, inherited from the part, then the package, then the defaults; kinds that no table describes are listed, not refused.
- **More names for the columns of a CSV position file,** extendable in the configuration, a choice between the centre and the origin of the part, and a setting for the angle of the bottom side.
- **Rotation corrections by package:** patterns with an angle added to the parts; the first matching rule wins.
- **Feeders with priority, trays and strips,** that can be switched off and warn when a tape will be nearly empty; the choice of the feeder does not depend on the order of the files.
- **Ranks:** blocks of ranks placed one after the other, with a preference inside a block.
- **Order strategies:** nearest, by kind, by height, by feeder, by nozzle tip, or by reference.
- **Time model of the moves** with top speed, acceleration and jerk per axis, X and Y moving together.
- **Shorter walks** with 2-opt moves that never make a walk longer.
- **Nozzle tips and angle limits:** a batch holds one tip, the tip changer costs its trip and its time, and the nozzle turns within its range, while the head moves or after.
- **Origin of a part:** the correction from the origin of the footprint to the centre of the pads, turned with the part.
- **Panels:** boards and panels inside panels, each with a place and a turn, fiducials, and stable names such as `A/R1`.
- **Load sheet:** the slot of each kind of tape (the widest tapes first, nearest to the board) as a CSV and as a file of feeders.
- **Exports:** a report, the list of what is missing and a G-code that only moves the head.
- **Checks before a job,** each with a severity and the part, feeder or setting it points at.
- **Simulated machine** with a virtual clock and failures injected by chance (with a seed) or at a chosen place, and traits for the machine, nozzles, feeders, an actuator, a camera and a driver.
- **Job:** pause, step by step, retries in layers, the policies alert and defer, feeders switched off after repeated faults, the rule that a nozzle never picks while it holds a part, statistics, and a saved state to resume from.
- **Commands** `schedule`, `preflight`, `simulate`, `loadsheet` and `config-check`.
- **Files larger than `max_file_bytes` are refused** and the number of parts is limited by the configuration.
- **A test checks** that the eight READMEs give the number of tests that exist.
- **One reader for the position file and the file of feeders:** comma or semicolon, fields in quotes that hold the delimiter, a doubled quote or a line break.
- **A big board is checked, assigned and planned in moments:** fifty thousand parts; the references and the feeders are looked up in an index and the walk over the places uses a grid, with the same result as before.
- **The plan is never written over a file it was made from.**
- **`--origin-x` for the bottom side:** the mirror is about the middle of the board whose left edge is at that X (0 by default, documented).
- **An unknown option or an extra word is an error** (the shared command line file), and the commands of the README are run by a test.

- **The window:** `phoenix-pnp` with no command (or with `gui FILES...`) opens an egui window when it is built with `--features gui`; with a command it stays a command line.
- **Tabs and tables:** the job (plan, batches and tip changes), parts, packages, feeders, load sheet, machine, problems and log; tables sort by a column, a part chosen in one is chosen in all with its feeder, and the count of a feeder is edited with a second click.
- **The map of the work table:** parts at their places with the polarity mark, feeders, the path of the head by batch, layers that can be switched, zoom and drag, and the part that comes next marked while the job runs.
- **Problems and fixes:** the checks before a job as a list; a part with no feeder can be left out and a feeder that is short can be given what it lacks, both can be undone, and the plan is made again after each.
- **A simulated job in the window:** start, pause, resume, one step and reset, with the events of the machine in the log; an error stops it and resume tries the step again.
- **A guide of five steps,** samples from a board of six parts to one of four hundred, a log with a diagnostic that holds no paths, and the plan, report and load sheet written to a folder (never over a file that was loaded).
- **The eight languages in the window,** with the error codes `PHOENIX-PNP-NNN` in the language chosen, three themes (phoenix, light and high contrast) and files that can be dropped on the window.
- **Rust 1.95 or newer** is needed now (the window's library asks for it, and so does the lock file).
- **Three keys of `machine.cfg`** for the window: `gui_steps_per_s`, `gui_theme` and `gui_part_size_mm`.
- **The CI also checks the window:** clippy runs with the `gui` feature, and the logic of the window is covered by tests that need no screen.

- **Renamed:** The project is now called EH3D-PHOENIX-PNP (it was EH3D-PNP-MANAGER); it joins the PHOENIX family of programs.

## [0.0.4] - Audit fixes, the new reading limits, tests and the application icon

- **Brand:** Add the application icon and the logos of the program.
- **Shared checks updated:** the validator also compares the badge and the Version row of the README with the manifest, fails when one of its checks cannot run, and the CHANGELOG step of the workflow reads the base branch from the environment.
- **Safer files:** a plan is written through a temporary file, text that is not UTF-8 is read as Latin-1 with a note, and a fractional `--nozzles` is refused.
- **One side at a time:** a plan for one side no longer asks feeders for the parts of the other side nor uses them up, and a position column whose header names its unit (`PosX(mil)`, `Center-X(in)`) reads bare numbers in that unit.
- **The workflows are hardened:** every action is pinned by its commit with its version in a comment, and the checkouts keep no credentials.
- **More tests:** the units and the delimiters of the position files, the rows that are wrong, the limit of parts, the KiCad position file and the file of feeders.

## [0.0.3] - CI hardening and shared validator

- **Faster, bounded CI:** the Cargo build is cached, a run that a newer push supersedes is cancelled, every job has a time limit, and the Rust jobs live in their own workflow that is skipped when only documents or images change.
- **A fixed Rust toolchain:** `rust-toolchain.toml` names the compiler version (with rustfmt and clippy) that the CI installs, so a new stable release cannot turn the lints red without a change here.
- **Monthly dependency updates:** Dependabot opens one grouped pull request a month for the GitHub Actions and the Cargo dependencies.
- **A line for every change:** a pull request that changes code must add a line under `[Unreleased]` of this file (`tools/changelog_pr_check.py`; the label `no-changelog` skips it).
- **Shared checks updated:** the validator of the suite checks the text of the banner against the manifest and reads the files that git tracks.

## [0.0.2] - More tests and a KiCad file with a byte order mark

- **Strict command line:** an option that is written wrong (`--powr`) or a word too many is an error that names it; before, it was ignored and the output was made with the default values.
- **The banner of the README** shows the role and the technologies of this program (it showed the text of the shared library).
- **More tests:** the position files are checked with a byte order mark, tabs, units and angles of any size; the feeders with exhausted tapes and non-ASCII names; the plan with exact travel and time, mirroring twice and the same result on two runs; and every command of the command line. A KiCad position file saved with a byte order mark is now read.

## [0.0.1] - Pick and place assembly: the position files, the feeders and the order of the placements

- **The parts:** KiCad `.pos` and CSV position files in mm, mil or inches, with the angles normalised and a damaged row refused with its number.
- **The feeders:** a CSV of feeders; every part gets the feeder of its value and package; parts without a feeder and feeders that run out are reported.
- **The plan:** nearest-first placements cut in batches of one to six nozzles, the bottom side mirrored, the travel and an estimate of the time, as a JSON job.
- **The command line** `eh3d-pnp` with `parts`, `check` and `plan`, and a sample board with its feeders.
- **14 tests** run on a computer, with damaged input.

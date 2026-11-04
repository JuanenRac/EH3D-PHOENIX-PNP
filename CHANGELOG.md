# Changelog

All notable changes to this project are documented here.

## [Unreleased]
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

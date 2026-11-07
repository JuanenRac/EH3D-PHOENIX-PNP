<p align="center">
  <img src="images/EH3D_PHOENIX_PNP_BANNER.svg" alt="EH3D-PHOENIX-PNP banner" width="100%">
</p>

# 📖 EH3D-PHOENIX-PNP

<p align="center">🇺🇸 <b>English</b> | <a href="README_spa.md">🇪🇸 Español</a> | <a href="README_fra.md">🇫🇷 Français</a> | <a href="README_ita.md">🇮🇹 Italiano</a> | <a href="README_deu.md">🇩🇪 Deutsch</a> | <a href="README_zho.md">🇨🇳 中文</a> | <a href="README_jpn.md">🇯🇵 日本語</a> | <a href="README_por.md">🇵🇹 Português</a></p>

### Pick and place assembly: positions, feeders and the order of the placements

<p align="center">
  <img src="https://img.shields.io/badge/License-GPL%203.0--or--later-blue.svg" alt="GPL 3.0 or later">
  <img src="https://img.shields.io/badge/Stack-Rust-DEA584.svg" alt="Rust">
  <img src="https://img.shields.io/badge/Languages-8-00E5FF.svg" alt="Languages">
  <img src="https://img.shields.io/badge/Version-0.0.6-0f6cbd.svg" alt="Version">
  <img src="https://img.shields.io/badge/Maturity-scaffolding-FFB020.svg" alt="Maturity">
</p>

---

**Honesty check - what actually runs today:** a Rust library and a command line program (`phoenix-pnp`) with 452 tests that run on a computer: reads the position file of a board, gives each part its feeder (with priorities, trays and strips), reads the settings of the machine from a file, plans the order of the placements for a head with up to six nozzles (ranks, nozzle tips, angle limits, time of the moves, panels), writes a load sheet and, on a simulated machine, carries the plan out step by step with injected failures. It also has a window (built with `--features gui`) whose logic is tested without a screen; the drawing itself is not covered by tests, and although the program takes pictures of its own window (`--shot`), no person has reviewed the drawing on a screen. It does not drive any machine: it has not been tried on a machine and it has not been checked against a collection of real files.

---

> **Safety and security status:** this program does not control any machine and is not a safety system. What it produces must be checked by a person before anything is run on a machine; nothing here has run on real hardware.


## 📋 At a glance

| | |
|---|---|
| Version | `0.0.6` |
| Maturity | Scaffolding |
| Tests | 452 |
| Stack | Rust |
| On real hardware | Not tried: nothing here has run on a machine |

**Jump to:** [Overview](#-overview) · [Architecture](#-architecture) · [Quick start](#-quick-start) · [Testing](#-testing) · [Structure](#-repository-structure) · [Related](#-related-projects)

## 🚦 Project status

Each line is one part of this repository and the light says how far it is today. It is written from what the repository can show (tests, builds, runs), not from what is planned.

🟢 Done and verified · 🟡 Works, with known limits · 🟠 Partly done or not verified yet · 🔴 Not done or not working

| Area | State | What it means here |
|---|---|---|
| Core | 🟡 | reads the position file of a board, gives each part its feeder (priority, tray, strip), plans the order of the placements for a head with up to six nozzles with ranks, nozzle tips, angle limits and the time of the moves, plans panels and writes the load sheet; tested on a computer with normal and damaged input, not with a collection of real files. |
| Command line | 🟡 | `phoenix-pnp` runs the same functions as the library (`parts`, `check`, `plan`, `schedule`, `preflight`, `simulate`, `loadsheet`, `config-check`, `check-bottom`, `tables-check`, `vision-test`) and prints the result; every error comes back as a message with its `PHOENIX-PNP-NNN` code and a non-zero exit code. |
| Tests | 🟡 | 452 tests pass (`cargo test`), with normal cases, edge cases, damaged input and small generated cases. |
| Machine configuration | 🟡 | a `key = value` file with a version, a validator that names the line, and the safety limits (travel of Z, limits of X and Y, retries, nozzle angles); the values have not been compared with a real machine. |
| Simulator and job | 🟡 | a virtual clock, failures injected by chance (with a seed) or at a chosen place, retries, the policies alert and defer, feeders that switch off after repeated faults, pause, step by step, and a saved state to resume from; it only simulates, nothing has driven a machine. |
| Security | 🟡 | Plain safe Rust (`forbid(unsafe_code)`): nothing panics on the files it is given and a bad value is an error with its reason; no independent review. |
| Window | 🟠 | An egui window (build it with `--features gui`; `phoenix-pnp` with no command opens it): tabs for the job, parts, packages, feeders, load sheet, machine, problems and log, a map of the work table with the path of the head, polarity marks and the part that is next, a guide of five steps, problems with fixes that can be undone, a simulated job that can be paused, resumed and stepped, and the eight languages. Its logic is tested without a screen; the drawing is not covered by tests: the program takes pictures of its own window (`--shot`) but no person has reviewed the drawing on a screen. It also has tabs for panels (with the fiducials and the fit that moves the parts) and boards, a file picker, tables of feeders and packages that can be edited with undo, column widths that are remembered, and a program of its own (`phoenix-pnp-gui`) that opens without a console on Windows. |
| Real hardware | 🔴 | Not tried: nothing here has been run on a machine. |
| Documentation | 🟡 | README in eight languages, the changelog and the two pictures drawn from `docs/`. |

## 🎯 Overview

* **The parts** (`src/parts.rs`): a KiCad `.pos` file or a CSV (comma or semicolon, quotes, a header found by name) with positions in mm, mil or inches, the angles brought to 0-360, the side top or bottom, and every row that makes no sense refused with its number; a repeated reference is an error.
* **Parts that are not to be mounted, the tables of the examples and the report of a job** (`src/parts.rs`, `src/commands.rs`, `src/job.rs`): a column `dnp`, `do not place` or `fitted` in a CSV (or `--dnp R1,R2`) leaves parts out of the plan, the load sheet and the checks and lists them in the report; `tables-check` reads the tables (`--parts`, `--packages`, `--catalog`, `--feeders`, `--slots`) and names the code and the row of every error; `simulate --report-csv FILE` writes the statistics of the job as a CSV whose header never changes. `examples/` has a board with a `dnp` column, feeders of every kind and a table of common packages.
* **The feeders** (`src/feeders.rs`): a CSV of feeders (id, value, package, pick position, parts left); every part gets the first feeder that holds its value and package, without regard to case; a part with no feeder and a feeder that runs out are reported, and a second feeder of the same kind takes over.
* **The plan** (`src/plan.rs`): the places are walked nearest-first and cut in batches of one to six nozzles, the feeders of a batch are visited nearest-first, the bottom side is mirrored about the width of the board, and the plan gives the travel and an estimate of the time; it is written as a JSON job.
* **The tables and the settings** (`src/catalog.rs`, `src/config.rs`, `src/rotation.rs`): tables of parts and packages whose values are inherited (part, then package, then the defaults), a machine configuration file in `key = value` format (`examples/machine.cfg` documents every key), more names for the columns of a CSV position file, and corrections of the rotation by package.
* **The feeders with more than a tape** (`src/feeding.rs`): priority, trays, strips and feeders that are switched off; the feeder of a part is chosen the same whatever the order of the files.
* **The planner of the machine** (`src/planner.rs`, `src/motion.rs`): ranks (blocks that are placed one after the other), strategies of order, nozzle tips and their changes, the limits of the angle of the nozzle, the time of the moves with acceleration and jerk, shorter walks (2-opt) and the correction of the origin of a part.
* **Panels, load sheet and exports** (`src/panel.rs`, `src/loadsheet.rs`, `src/export.rs`): boards and panels inside panels, the slot of each kind of tape, a report, the list of what is missing and a G-code that only moves the head.
* **Fiducials of a panel** (`src/fiducial.rs`): the measured places of the marks are fitted to the designed ones (a similarity with two marks, an affine fit with three or more) and the parts of the panel are moved and turned by the fit; each mark has its residual, and a fit that misses a mark by more than `fiducial_max_residual_mm` is refused (`--fiducials FILE` with `--panel`, or the Panels tab of the window).
* **Boards marked as bad (X-out) and the marks of two levels** (`src/marks.rs`, `src/panel.rs`): a child of the panel file with `"x_out": true`, or `--x-out A,B/C`, is left out with everything inside it; the plan, the load sheet and the checks do not see it, and a name that no board has (`PHOENIX-PNP-042`) or a panel with every board left out (`PHOENIX-PNP-043`) is an error. A part of a board whose value or package holds a word of `fiducial_words`, or whose reference is a prefix of `fiducial_refs` followed by a number (`FID1`), is a mark of its board and is not placed; the file of `--fiducials` can then name the marks of the panel (`f1`) and of the boards (`A/FID1`): the marks of a board are fitted together and move only the parts of that board, and the parts of the boards without measured marks follow the fit of the panel. Simulated: no board was ever measured on a machine; the window does not use the marks of the boards yet.
* **More from the planner and the simulator:** the order in which the cells of a tray are emptied (rows, columns or a snake), a search for the order of the nozzle tips that counts the way and the trip to the changer (`tip_strategy = optimize`, limited by `tip_opt_max_evals` and `tip_opt_time_limit_s`), chances of failure by package and by feeder (`sim_part_fail`, `sim_feeder_fail`) with statistics by package and by feeder and the parts per hour, and `check-bottom`, which shows the angle that each rule of the bottom side gives.
* **The align step** (`src/job.rs`, `src/sim.rs`, `src/model.rs`): with `align = on` the job looks at each part with the camera after the pick and corrects the place by what it sees (the head goes to the place less the offset of the part, and the nozzle turns less the turn of the part); `pick_tolerance_mm` throws away a part that is further off, which is picked again as `error_policy` says; `align_passes` (1 to 5) and `align_settle_s` set the looks and the wait before each one, which add to the time of the plan; the simulated camera has an offset, an error and failures that can be injected (`sim_align_*`, `sim_camera_fail_rate`); and the correction (mean, largest, parts thrown away) is in the text, in the JSON report and in the `align` rows of `--report-csv`. Off by default: a job is then what it was. Not tried on a machine.
* **The vision pipeline** (`src/vision.rs`, `src/sim.rs`, `examples/vision.cfg`): with `vision = pipeline` the simulated camera draws a picture of the part in memory (its size, its turn and how far it is from the nozzle, with a noise drawn from `sim_seed`) and measures it with the steps of a file of settings (`--vision FILE`): a mask, a median filter, a fixed or Otsu threshold, a range of area for the spot and the smallest turned rectangle, whose middle and angle are the offset that the align step corrects. A part that is not found, a spot of a wrong area and several spots are looks without a reading, each with its own code (`PHOENIX-PNP-037` to `PHOENIX-PNP-039`). `phoenix-pnp vision-test` draws a part from the size, turn, offset and noise that it is given and prints what was measured with its error. Off by default: a job is then what it was. Not tried with a real camera.
* **The work report** (`src/report.rs`, `src/job.rs`): after a job is simulated, `simulate` can write the report as text in the language of `--lang` (`--report-text`), as a page of HTML that holds everything it shows, with no script and nothing from outside, and that is never written over a file that exists (`--report-html`), as sections added after the rows of the CSV (`--report-csv`, the header does not change) and as `work` in the JSON (`--report`). It says how many parts were placed, put aside, left out by the person, still pending or not to be mounted; where the machine time went (feed and pick, the looks of the camera, the places, the moves of the head, the changes of tip); how many picks worked at the first attempt; the same by package, feeder, nozzle, nozzle tip and board of a panel; the errors that were raised with their codes; the findings before the job; and a histogram of what the camera corrected (X, Y and turn). Simulated: nothing in it was measured on a machine.
* **The Job tab and the decisions of the person** (`src/gui/jobtab.rs`, `src/gui/window.rs`, `src/job.rs`): the table shows the state of every placement (waiting, picked, aligned, placed, put aside, left out, failed) and the parts not to be mounted, a bar of progress with the machine time and an estimate of the time left, and filters by state, by text and by what is done. When an error stops the job a dialog names the step that failed and offers four actions that are carried out on the simulated job, which then goes on: retry, skip the part, put it aside, or switch the feeder off so that another feeder of the same kind takes over. Each decision is counted in the report. The logic needs no screen and is tested; no person has reviewed the drawing on a screen (the pictures of this tab in `images/screens/` are taken by the program itself).
* **The simulator and the job** (`src/sim.rs`, `src/job.rs`, `src/preflight.rs`, `src/model.rs`): the checks before a job, a machine with a virtual clock and failures that can be injected, and a job that can be paused, taken step by step, saved and resumed.
* **The window** (`src/gui.rs`, `src/gui/`): what the window does besides drawing needs no screen (the files that were loaded, the plan and its problems with fixes, the simulated job, the geometry of the map, the tables with a shared selection, the guide of five steps and the words in eight languages) and is tested; only `src/gui/window.rs`, built with `--features gui`, draws it with egui. The speed of the simulation, the theme and the size drawn for a part of an unknown package are keys of `machine.cfg` (`gui_steps_per_s`, `gui_theme`, `gui_part_size_mm`).

## 🧩 Role in EH3D

EH3D-PHOENIX-PNP is one program of the EH3D Fabrication Suite. Today it works as a library, a command line and a window (built with `--features gui`); the window does not yet have the look the other programs of the suite share. It reads and writes plain files, so it can be used on its own or between the other programs.

## 🏛️ Architecture

<p align="center">
  <img src="images/architecture.svg" alt="EH3D-PHOENIX-PNP architecture: who uses it, what it holds and what it depends on" width="100%">
</p>


How the parts of this repository fit together: who uses it, what it holds and what it depends on. The picture is drawn from `docs/architecture.json` and checked against the code.

## ✨ Features

* **A library and a command line:** every command of `phoenix-pnp` is a function of the library, so another program of the suite can call it without starting a process.
* **Strict command line:** an option that is written wrong (`--powr`) or a word too many is an error that names it, not something to ignore; the program would otherwise make its output with the default values.
* **Tests with damaged input:** the tests also feed empty files, garbage, huge numbers and cut files, and none of them may panic.
* **No guessing:** a value that does not make sense is an error that says which value and why, never a result that only looks right.
* **No unsafe code:** `#![forbid(unsafe_code)]` in the library and the program.
* **One reader for both tables, and fast on a big board:** the position file and the file of feeders are read by the same reader (comma or semicolon, fields in quotes with a delimiter or a line break), fifty thousand parts are checked, assigned and planned in moments, the plan never goes over a file it was made from, an unknown option is an error, and `--origin-x` says where the left edge of the board is when the bottom side is mirrored.

## 🔒 Security & Safety

| Threat | Defense |
|---|---|
| A damaged, truncated or empty file | no panic: the tests cut valid input at every position and feed garbage |
| A number that makes no sense (negative, not finite, absurdly large) | refused with an error that names the value; nothing is rounded into something plausible |
| Unsafe memory use | plain safe Rust with `forbid(unsafe_code)` |

**What is not defended:** a result that was produced cleanly is not a verified result. Nothing here knows the limits of a machine, and the program has had no independent review. Report a vulnerability privately (see [SECURITY.md](SECURITY.md)).

## 🔌 Interfaces

The library is `eh3d_phoenix_pnp` (`use eh3d_phoenix_pnp::...`); the command line is `phoenix-pnp`. The commands are these; each prints its result on the standard output and reports an error on the standard error with a non-zero exit code. There is no network access.

```text
phoenix-pnp [gui [FILES...]]                             the window (built with --features gui)
phoenix-pnp parts PARTS                                  the parts of a position file, in millimetres
phoenix-pnp check PARTS FEEDERS                          the parts with no feeder and the feeders that will run out
phoenix-pnp plan PARTS FEEDERS OUT.json                  the simple plan, as a JSON job
phoenix-pnp schedule PARTS FEEDERS OUT.json              the plan with the whole configuration, panels and fiducials
phoenix-pnp preflight PARTS FEEDERS                      the checks before a job
phoenix-pnp simulate PARTS FEEDERS                       the job on a simulated machine, with its reports
phoenix-pnp loadsheet PARTS SLOTS OUT.csv                the load sheet
phoenix-pnp config-check FILE                            whether a configuration file is valid
phoenix-pnp check-bottom PARTS                           the angle each rule of the bottom side gives
phoenix-pnp tables-check [--parts F] [--feeders F] ...   whether each table is valid, with the code and the row
phoenix-pnp vision-test [--turn DEG] [--dx MM] ...       measures the picture of a part made in memory
```

`phoenix-pnp help` lists every option. The options shared by `schedule`, `preflight`, `simulate` and `loadsheet` are `--config`, `--packages`, `--catalog`, `--panel`, `--fiducials`, `--x-out`, `--dnp`, `--side`, `--board-width`, `--origin-x`, `--nozzles`, `--speed`, `--order` and `--lang`; `--shot` and its companions take a picture of the window (see Quick start).

## ⚙️ Configuration

The machine and the planner are configured with a text file of `key = value` lines (`--config FILE`), the same format as the other programs of the suite; `examples/machine.cfg` lists every key with its default and what it does. A key that is missing keeps its default; a key that is not known, written twice or outside its range is an error that names the line. The options of the command line (`--nozzles`, `--speed`, `--order`, `--side`) win over the file.

The columns that mark a part as not to be mounted and the words they use are settings too: `header_dnp` and `header_fitted` add names for the column, `dnp_yes_words` and `dnp_no_words` add words for yes and no. A column called `dnp`, `do not place`, `do not populate`, `do not mount`, `do_not_place`, `not fitted` or `no montar` leaves the part out with a yes (`yes`, `y`, `true`, `1`, `x`); one called `fitted`, `populate`, `populated`, `mount`, `mounted` or `montar` leaves it out with a no (`no`, `n`, `false`, `0`, `-`); an empty cell mounts the part and any other word is an error that names the row.

The parts that are marks of their board are settings too: `fiducial_words` (default `fiducial`) and `fiducial_refs` (default `fid, fd`); an empty list turns the rule off.

The tables of packages and parts (`--packages`, `--catalog`) are CSV files in which an empty cell inherits; a panel (`--panel`) is a JSON file; the slots of the machine (`loadsheet`) are a CSV with `id,x,y,max_width_mm`. A file larger than `max_file_bytes` is refused. Every error has a code `PHOENIX-PNP-NNN`; its message is written in the language of the key `language` or of `--lang` (`en`, `es`, `de`, `fr`, `it`, `ja`, `zh` or `pt`):

| Code | Meaning |
|---|---|
| `PHOENIX-PNP-001` | The file is larger than the allowed size |
| `PHOENIX-PNP-002` | There are more parts than the limit allows |
| `PHOENIX-PNP-003` | A line of the configuration is not key = value |
| `PHOENIX-PNP-004` | The configuration has a key that is not known |
| `PHOENIX-PNP-005` | A value of the configuration is not valid |
| `PHOENIX-PNP-006` | A value of the configuration is outside its range |
| `PHOENIX-PNP-007` | The configuration was written for a newer version of the program |
| `PHOENIX-PNP-008` | A key of the configuration is written twice |
| `PHOENIX-PNP-009` | A setting is beyond the safety limits |
| `PHOENIX-PNP-010` | A row of the table of parts or packages is not valid |
| `PHOENIX-PNP-011` | A part or package is defined twice |
| `PHOENIX-PNP-012` | A rule of rotation offset is not valid |
| `PHOENIX-PNP-013` | A row of the feeders is not valid |
| `PHOENIX-PNP-014` | The panel file is not valid |
| `PHOENIX-PNP-015` | The panel is nested too deep or has too many boards |
| `PHOENIX-PNP-016` | The table of slots is not valid |
| `PHOENIX-PNP-017` | A setting of the plan is not valid |
| `PHOENIX-PNP-018` | The nozzle cannot turn to that angle within its limits |
| `PHOENIX-PNP-019` | The nozzle already holds a part: it cannot pick another |
| `PHOENIX-PNP-020` | The nozzle holds no part to place |
| `PHOENIX-PNP-021` | No feeder is available for the part |
| `PHOENIX-PNP-022` | The part could not be picked after all the attempts |
| `PHOENIX-PNP-023` | The feeder could not advance after all the attempts |
| `PHOENIX-PNP-024` | The part could not be placed after all the attempts |
| `PHOENIX-PNP-025` | The saved state belongs to another job |
| `PHOENIX-PNP-026` | The saved state of the job is not valid |
| `PHOENIX-PNP-027` | The job is paused, finished or stopped by an error |
| `PHOENIX-PNP-028` | The checks before the job found errors |
| `PHOENIX-PNP-029` | A length or an angle is not valid |
| `PHOENIX-PNP-030` | The driver did not accept the command |
| `PHOENIX-PNP-031` | The simulator injected a fault |
| `PHOENIX-PNP-032` | The fiducials are not valid or do not give a position |
| `PHOENIX-PNP-033` | A mark of mounting is not yes or no, or a reference to leave out is not known |
| `PHOENIX-PNP-034` | The file of positions is not valid |
| `PHOENIX-PNP-035` | The camera found the part further out of place than the tolerance allows |
| `PHOENIX-PNP-036` | The camera gave no reading of the part |
| `PHOENIX-PNP-037` | The vision did not find the part in the image |
| `PHOENIX-PNP-038` | The spot found in the image has an area outside the allowed range |
| `PHOENIX-PNP-039` | The vision found more than one spot that could be the part |
| `PHOENIX-PNP-040` | The report file already exists and is not written over |
| `PHOENIX-PNP-041` | The action does not fit the state of the job |
| `PHOENIX-PNP-042` | A board marked as bad (X-out) is not in the panel |
| `PHOENIX-PNP-043` | Every board of the panel is marked as bad (X-out): nothing is left to place |

## 🚀 Quick start

You need a stable Rust toolchain (1.95 or newer).

```bash
cargo build --release
cargo test
cargo run --release -- plan examples/board.pos examples/feeders.csv examples/plan.json --nozzles 2
cargo run --release -- schedule examples/board.pos examples/feeders.csv examples/schedule.json --config examples/machine.cfg --packages examples/packages.csv --catalog examples/catalog.csv
cargo run --release -- simulate examples/board.pos examples/feeders.csv --config examples/machine.cfg --state examples/job_state.json --steps 5
cargo run --release -- loadsheet examples/board.pos examples/slots.csv examples/loadsheet.csv --packages examples/packages.csv
cargo run --release -- check-bottom examples/board.pos
cargo run --release -- schedule examples/board_dnp.csv examples/feeders.csv examples/schedule_dnp.json --config examples/machine.cfg --packages examples/packages.csv --catalog examples/catalog.csv
cargo run --release -- tables-check --parts examples/board_dnp.csv --packages examples/packages_common.csv --feeders examples/feeders_kinds.csv --slots examples/slots.csv --config examples/machine.cfg
cargo run --release -- simulate examples/board.pos examples/feeders.csv --config examples/machine.cfg --report-csv examples/job_report.csv
cargo run --release -- simulate examples/board.pos examples/feeders.csv --config examples/machine.cfg --report-text examples/job_report.txt --report-html examples/job_report.html
cargo run --release -- schedule examples/feeders.csv examples/schedule_panel.json --panel examples/panel.json --fiducials examples/marks.csv --config examples/machine.cfg
cargo run --release -- schedule examples/feeders.csv examples/schedule_xout.json --panel examples/panel.json --x-out B --config examples/machine.cfg
cargo run --release -- vision-test --vision examples/vision.cfg --turn 30 --dx 0.5 --dy -0.3
cargo run --release --features gui -- --shot screen.png examples/board.pos examples/feeders.csv examples/machine.cfg   # a picture of the window
cargo run --release --features gui -- --shot job.png --window job --steps 9 examples/board.pos examples/feeders.csv examples/machine.cfg   # the Job tab with the job on its way
cargo run --release --features gui
```

To take a picture of the window without opening it by hand, run it with `--shot FILE.png` (also `--size WxH`, `--theme phoenix|light|contrast`, `--wait N`, `--window TAB`, `--steps N`, `--lang CODE`, `--open FILE`, `--replace`): it opens at that size, saves the PNG, keeps no settings and closes by itself (exit code 0 when the file was written, 1 when it was not, 2 for wrong arguments). `--window` chooses the tab of the picture (`job`, `parts`, `packages`, `feeders`, `panels`, `boards`, `loadsheet`, `machine`, `problems` or `log`) and `--steps N` makes N steps of the simulated job before it is taken, so that the Job tab shows a job on its way.

<p align="center">
  <img src="images/screens/job_en.png" alt="The Job tab with the simulated job on its way" width="100%">
</p>

The last line opens the window; so does `phoenix-pnp` with no command when it was built with `--features gui` (on Linux the window needs the development files of X11 or Wayland and OpenGL). With a command, the program stays a command line. On Windows `phoenix-pnp-gui` (built with the same feature) opens the window without a console; see `docs/windows-console.md`.

## 🧪 Testing

The suite has 452 tests (`cargo test`, run on Windows and Linux in the CI). They check the normal cases, the edge cases and damaged input: empty files, garbage and cut files never panic. Small generated cases (from a fixed seed, so they give the same numbers on every run) check that the plan does not depend on the order of the files, that a walk is never made longer, that a job saved and resumed anywhere ends as the one that never stopped, and that lengths and angles convert back and forth. The CI also runs `cargo fmt --check`, `cargo clippy` with warnings as errors and `python -I tools/ci_validate.py`.

## 🏗️ Build & Release

The version lives in `Cargo.toml` and in `eh3d.project.json`; each version has its entry in [CHANGELOG.md](CHANGELOG.md). `build.bat` / `build.sh` build the program, `test.bat` / `test.sh` run the tests and `run.bat` / `run.sh` run it with a sample. Releases are published on GitHub.

## 📂 Repository Structure

<p align="center">
  <img src="images/repository-structure.svg" alt="Repository structure of EH3D-PHOENIX-PNP" width="100%">
</p>

<details>
<summary>Text version of the tree</summary>

```text
EH3D-PHOENIX-PNP/
├── src/
│   ├── catalog.rs                               the tables of parts and packages and the inheritance of their properties
│   ├── cli_args.rs                              the arguments of the command line, strict: an unknown option is an error
│   ├── commands.rs                              the commands that use the configuration, tables-check and check-bottom
│   ├── config.rs                                the configuration file of the machine: keys, ranges and safety limits
│   ├── error.rs                                 the error codes PHOENIX-PNP-NNN and their meaning in eight languages
│   ├── export.rs                                the report, the list of what is missing and the G-code of the movements
│   ├── feeders.rs                               a part of the logic
│   ├── feeding.rs                               feeders with priority, trays and strips, and the choice of the feeder for a part
│   ├── fiducial.rs                              fiducials of a panel: the fit of the measured marks and the correction of the parts
│   ├── gui.rs                                   the window: modules that need no screen, and the drawing in window.rs
│   ├── gui/app.rs                               the state of the window: load, plan, start, pause, step, fix, save
│   ├── gui/browse.rs                            the file picker: folders and files, up, typed paths
│   ├── gui/jobtab.rs                            the Job tab without the window: states, filters, progress, error dialog
│   ├── gui/log.rs                               the log with levels, and the text of the diagnostic that can be copied
│   ├── gui/problems.rs                          the problems of a job and the fixes that put them right
│   ├── gui/samples.rs                           samples, from a small board to a large one
│   ├── gui/session.rs                           the simulated job that is started, paused, resumed and stepped
│   ├── gui/settings.rs                          what the window remembers: language, theme, layers, folders and the width of the columns
│   ├── gui/shot.rs                              the picture mode of the window (--shot): its arguments
│   ├── gui/tables.rs                            the rows, the sorting and the selection shared by the tables
│   ├── gui/texts.rs                             every word of the window in the eight languages
│   ├── gui/theme.rs                             the colours: phoenix green, light and high contrast
│   ├── gui/view.rs                              the geometry of the map: bodies, polarity marks, fit, zoom and hit
│   ├── gui/window.rs                            the drawing with egui, built with the gui feature: tabs, map and tables
│   ├── gui/wizard.rs                            the guide of five steps from nothing to a simulated job
│   ├── gui/workspace.rs                         the files that were loaded, the plan and the edits, without a window
│   ├── gui_main.rs                              the window as a program of its own, without a console on Windows
│   ├── job.rs                                   the job: steps, pause, retries, policies, statistics and saved state
│   ├── lib.rs                                   the library: what every command uses
│   ├── loadsheet.rs                             the load sheet: the slot of each kind of tape
│   ├── marks.rs                                 marks of two levels: the parts that are marks, the fit of a panel and of each board
│   ├── main.rs                                  the command line, and the entry of the window when it runs with no command
│   ├── model.rs                                 the parts of a machine as traits: machine, nozzle, feeder, actuator, camera, driver
│   ├── motion.rs                                the time a move takes, with acceleration and jerk
│   ├── panel.rs                                 panels: boards and panels inside panels, each in its place
│   ├── parts.rs                                 the position file and the parts that are not to be mounted
│   ├── plan.rs                                  a part of the logic
│   ├── planner.rs                               the planner that knows the machine: ranks, order, tips and their search, angles and time
│   ├── preflight.rs                             the checks before a job, with a severity and what they point at
│   ├── report.rs                                the report of a job: phases, picks, groups, errors; text, HTML, CSV, JSON
│   ├── rotation.rs                              corrections of the rotation by package, and how a limited nozzle turns
│   ├── sim.rs                                   the simulated machine: virtual clock and failures that can be injected
│   ├── units.rs                                 lengths and places that carry their unit
│   └── vision.rs                                the vision pipeline: filter, threshold, spots and the smallest turned rectangle
├── tests/
│   ├── align.rs                                 tests of the align step: camera, corrected place, tolerance and time
│   ├── commands.rs                              tests of the commands that use the configuration
│   ├── feeding.rs                               tests of the feeders: priority, trays and strips
│   ├── gui.rs                                   tests of the window without a screen: state, plan, job, map, tables and guide
│   ├── job.rs                                   tests of the job: steps, retries and saved state
│   ├── jobtab.rs                                tests of the Job tab without a screen: states, filters, progress, dialog
│   ├── loadsheet.rs                             tests of the load sheet and the exports
│   ├── marks.rs                                 tests of the boards marked as bad, the parts that are marks and the marks of two levels
│   ├── model.rs                                 tests of the data model: codes, lengths, configuration and tables
│   ├── more.rs                                  more tests of the library and the command line
│   ├── panel.rs                                 tests of panels and of their fiducials
│   ├── planner.rs                               tests of the time of the moves and of the planner
│   ├── pnp.rs                                   tests of the library and the command line
│   ├── preflight.rs                             tests of the checks before a job
│   ├── readme.rs                                tests that the README can be run and is complete
│   ├── report.rs                                tests of the job report: phases, picks, histograms, groups, decisions, formats
│   ├── shot.rs                                  tests of the picture mode of the window: its arguments, read and checked without a screen
│   ├── sim.rs                                   tests of the simulated machine
│   ├── strict.rs                                tests of the command line: options and words it does not know
│   ├── tables.rs                                tests of the parts not to be mounted, of tables-check and of the report of a job as a CSV
│   ├── vision.rs                                tests of the vision pipeline: pictures checked by hand, failures, settings
│   └── vision_job.rs                            tests of the vision in the job and vision-test: readings, failures, saved state
├── examples/
│   ├── board.pos                                a sample to try the program with
│   ├── board_dnp.csv                            a board with a dnp column: two of its parts are not to be mounted
│   ├── catalog.csv                              a table of parts: rank and height
│   ├── feeders.csv                              a sample to try the program with
│   ├── feeders_kinds.csv                        feeders of every kind: tape, strip, tray and one switched off
│   ├── machine.cfg                              the settings of the machine, every key explained
│   ├── marks.csv                                the measured places of the fiducials of the panel
│   ├── packages.csv                             a table of packages: height, nozzle tip, rank and tape
│   ├── packages_common.csv                      a table of common packages, from 0201 resistors to QFN, to adjust to your own parts
│   ├── panel.json                               a panel of two boards
│   ├── slots.csv                                the slots of the machine
│   └── vision.cfg                               the settings of the vision pipeline, every key explained
├── docs/
│   ├── architecture.json                        data of the Architecture picture
│   ├── structure.json                           data of the Repository Structure picture
│   └── windows-console.md                       why the window is a program of its own on Windows
├── tools/                                       CI check, README check and the picture tools
├── images/                                      banner, Architecture and Structure pictures, and the pictures of the window
├── .github/                                     CI and release workflows, issue and pull-request templates
├── eh3d.project.json                            manifest: version, role, maturity and build command
├── Cargo.toml Cargo.lock                        the workspace and its locked dependencies
├── rust-toolchain.toml                          the pinned Rust toolchain
├── CHANGELOG.md                                 what changed in each version
├── SECURITY.md SUPPORT.md                       how to report a vulnerability and where to ask
└── CONTRIBUTING.md CODE_OF_CONDUCT.md LICENSE   how to contribute, the code of conduct and the licence
```

</details>

## 🗺️ Roadmap & Known limits

**Known limits today**

* The drawing of the window has not been reviewed by a person on a screen (the pictures in `images/screens/` are taken by the program itself with `--shot`); the window has no jog and no real camera, and it does not yet use the marks that are parts of a board.
* It has not been tried on a machine nor with a collection of real files, and it has had no independent review; everything it does with a machine is a simulation.
* It does not read the files of other pick and place programs, and the rule for the angle of the bottom side of a CSV (`bottom_rotation`) has not been compared with real files of any CAD program.
* The camera and the vision are simulated: there is no real camera, no correction of the pick from a real picture and no driver of a real machine; the plan is a JSON file for another program to run.

**Next**

* Bring the window to the look shared by the programs of the suite, and make it use the marks that are parts of a board and show the boards marked as bad.
* Try the vision pipeline with pictures of a real camera and the marks of two levels with a real panel; send a plan to a machine only after trying it on a real one.
* Try it with real files and with the other programs of the suite.

## 🌍 Languages

README and texts exist in English, Spanish, French, Italian, German, Chinese, Japanese and European Portuguese.

## 🔗 Related Projects

* [EH3D-VISION-STUDIO](https://github.com/JuanenRac/EH3D-VISION-STUDIO): cameras, fiducials and inspection
* [EH3D-MOTIONCNC](https://github.com/JuanenRac/EH3D-MOTIONCNC): the machine control program
* [EH3D-GERBERVIEWER](https://github.com/JuanenRac/EH3D-GERBERVIEWER): the viewer of Gerber layers
* [EH3D-TOOL-MANAGER](https://github.com/JuanenRac/EH3D-TOOL-MANAGER): the tool crib shared by the CAM programs
* [EH3D-SUITE-LAUNCHER](https://github.com/JuanenRac/EH3D-SUITE-LAUNCHER): the hub that finds and starts the programs of the suite

## 📚 Documentation

* [Changelog](CHANGELOG.md)
* [Architecture and structure data](docs/architecture.json): the data the two pictures are drawn from.
* [Why the window is a program of its own on Windows](docs/windows-console.md)

## 🤝 Contributing & Community

* Before a pull request run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` and `python -I tools/ci_validate.py`: all must pass. See [CONTRIBUTING.md](CONTRIBUTING.md) and the [code of conduct](CODE_OF_CONDUCT.md).
* The README exists in eight languages: a change to the text is made in all eight files, and the two pictures are drawn from `docs/architecture.json` and `docs/structure.json` by `tools/make_architecture.py` and `tools/make_structure.py`.
* Report a vulnerability privately: [SECURITY.md](SECURITY.md). Questions, ideas and reports: electrohobby3d@gmail.com ([SUPPORT.md](SUPPORT.md)).

## 👤 AUTHOR

**JuanenRac (Electro Hobby 3D)** · electrohobby3d@gmail.com

## 📜 LICENSE

Software: GPL-3.0-or-later. See [LICENSE](LICENSE). Hardware designs of the suite use CERN OHL-S.

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
  <img src="https://img.shields.io/badge/Version-0.0.9-0f6cbd.svg" alt="Version">
  <img src="https://img.shields.io/badge/Maturity-scaffolding-FFB020.svg" alt="Maturity">
</p>

---

**Honesty check - what actually runs today:** a Rust library and a command line program (`phoenix-pnp`) with 889 tests that run on a computer: reads the position file of a board, gives each part its feeder (with priorities, trays and strips), reads the settings of the machine from a file, plans the order of the placements for a head with up to six nozzles (ranks, nozzle tips, angle limits, time of the moves, panels), writes a load sheet and, on a simulated machine, carries the plan out step by step with injected failures. It also has a window (built with `--features gui`) whose logic is tested without a screen; the drawing itself is not covered by tests, and although the program takes pictures of its own window (`--shot`), no person has reviewed the drawing on a screen. It does not drive any machine: it has not been tried on a machine and it has not been checked against a collection of real files.

---

> **Safety and security status:** this program does not control any machine and is not a safety system. What it produces must be checked by a person before anything is run on a machine; nothing here has run on real hardware.


## 📋 At a glance

| | |
|---|---|
| Version | `0.0.9` |
| Maturity | Scaffolding |
| Tests | 889 |
| Stack | Rust |
| On real hardware | Not tried: nothing here has run on a machine |

**Jump to:** [Overview](#-overview) · [Architecture](#-architecture) · [Quick start](#-quick-start) · [Testing](#-testing) · [Structure](#-repository-structure) · [Related](#-related-projects)

## 🚦 Project status

Each line is one part of this repository and the light says how far it is today. It is written from what the repository can show (tests, builds, runs), not from what is planned.

🟢 Done and verified · 🟡 Works, with known limits · 🟠 Partly done or not verified yet · 🔴 Not done or not working

| Area | State | What it means here |
|---|---|---|
| Core | 🟡 | reads the position file of a board, gives each part its feeder (priority, tray, strip), plans the order of the placements for a head with up to six nozzles with ranks, nozzle tips, angle limits and the time of the moves, plans panels and writes the load sheet; tested on a computer with normal and damaged input, not with a collection of real files. |
| Command line | 🟡 | `phoenix-pnp` runs the same functions as the library (`parts`, `check`, `plan`, `schedule`, `preflight`, `simulate`, `loadsheet`, `config-check`, `check-bottom`, `tables-check`, `recipe-check`, `import-centroid`, `vision-test`) and prints the result; every error comes back as a message with its `PHOENIX-PNP-NNN` code and a non-zero exit code. |
| Tests | 🟡 | 889 tests pass (`cargo test`), with normal cases, edge cases, damaged input and small generated cases. |
| Machine configuration | 🟡 | a `key = value` file with a version, a validator that names the line, and the safety limits (travel of Z, limits of X and Y, retries, nozzle angles); the values have not been compared with a real machine. |
| Simulator and job | 🟡 | a virtual clock, failures injected by chance (with a seed) or at a chosen place, retries, the policies alert and defer, feeders that switch off after repeated faults, pause, step by step, and a saved state to resume from; it only simulates, nothing has driven a machine. |
| Security | 🟡 | Plain safe Rust (`forbid(unsafe_code)`): nothing panics on the files it is given and a bad value is an error with its reason; no independent review. |
| Window | 🟠 | An egui window (build it with `--features gui`; `phoenix-pnp` with no command opens it): tabs for the job, parts, packages, feeders, load sheet, machine, tools, problems and log, a map of the work table with the path of the head, polarity marks and the part that is next, a guide of five steps, problems with fixes that can be undone, a simulated job that can be paused, resumed and stepped, and the eight languages. Its logic is tested without a screen; the drawing is not covered by tests: the program takes pictures of its own window (`--shot`) but no person has reviewed the drawing on a screen. It also has tabs for panels (with the fiducials and the fit that moves the parts) and boards, a file picker, tables of feeders and packages that can be edited with undo, column widths that are remembered, and a program of its own (`phoenix-pnp-gui`) that opens without a console on Windows. |
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
* **The vacuum, the actuators and the window of faults of a feeder** (`src/sim.rs`, `src/job.rs`, `src/config.rs`): with `vacuum = on` the simulated pump sucks for `vacuum_suck_s` seconds after each pick and the level of the nozzle (kilopascals) is read; a level under `vacuum_pick_min_kpa` means the nozzle holds no part (`PHOENIX-PNP-044`, and the pick is tried again as the retries say) and, after the blow of `vacuum_blow_s` seconds at each place, a level over `vacuum_release_max_kpa` means the part is still stuck to the nozzle (`PHOENIX-PNP-045`). The sensor has a generator of its own (`sim_vacuum_noise_kpa`, `sim_vacuum_miss_rate`, `sim_vacuum_stuck_rate`) and is off by default, so a job without it is exactly what it was. On/off devices are declared with `actuator = NAME : DELAY_S` and switched with `Job::set_actuator`, which adds their delay to the clock (`PHOENIX-PNP-046` for a name the machine does not have). With `fault_action = alert` a feeder that reaches `fault_limit` faults among its last `fault_window` picks no longer switches itself off: the job stops (`PHOENIX-PNP-047`) and the dialog of the Job tab offers to try again, leave the part out, put it aside or switch the feeder off.
* **The camera model, the heads of the machine and the nozzle tips of a package** (`src/camera.rs`, `src/heads.rs`, `src/tips.rs`, `src/config.rs`): the camera is described by `camera_units_per_px` (0 is the size the pipeline is set to), `camera_offset_x_mm` and `camera_offset_y_mm` (where the axis of the nozzle is in the picture), `camera_rotation_deg`, `camera_flip_x` and `camera_flip_y`, and the lens (`camera_lens_scale`, `camera_lens_k1`); the simulated camera of the align step reads the part through it, with or without a picture, and with the default values a job is exactly what it was. `head = NAME : NOZZLES [: OFFSET_X : OFFSET_Y : CAMERA_X : CAMERA_Y : CAMERA_ROT : CAMERA_UNITS : CAMERA_LENS]` declares up to four heads, each with its nozzles, its offset and its camera; with two or more heads `schedule` gives every part to a head that reaches it (`PHOENIX-PNP-048` when none does), plans each head and counts, on the clock of the machine, the time a head waits for a feeder that another head is using. `tip_compat = PATTERN : TIP, TIP, ...` lists the tips that a package accepts and `tip_compat_mode` (`fewest_changes` or `preferred`) chooses one tip for every kind of part so that the machine changes tip as few times as it can; a tip named in the tables is kept.
* **Recipes** (`src/recipe.rs`, `src/job.rs`, `src/commands.rs`): a text file with a version (`examples/job.recipe`) fixes the steps a job goes through (`prepare`, `pick`, `align`, `place` and `check`, in that order and none twice; the align step and the vacuum check are on only when the recipe has them) and the parameters that change for a package, a feeder or a nozzle tip (`pick_retries`, `pick_tolerance_mm`, `vacuum_suck_s` and `vacuum_blow_s`; for each key the first rule that matches wins, the feeders first, then the packages and then the tips). `phoenix-pnp recipe-check FILE` reads and checks it and `simulate ... --recipe FILE` carries the job out with it. Its errors have the codes `PHOENIX-PNP-049` to `PHOENIX-PNP-052` (a line that is not valid, steps out of order, a file over its limits, a version that is too new) and the limits are the keys `recipe_max_bytes` and `recipe_max_rules` of `machine.cfg`. A saved state also remembers the feeder of the last pick and the pick that was stopped, so a job loaded in another run offers the same decisions to the person (an older state still loads).
* **Import of position files** (`src/import.rs`, `src/commands.rs`): `phoenix-pnp import-centroid FILE` reads a centroid CSV (`--map FILE` names its columns, `examples/centroid.map`; `examples/centroid.csv` needs no map), the text of positions of EAGLE (`examples/board.mnt`: name, x, y, rotation `R90` or `MR90` for the bottom side, value, package) or the `.pos` of KiCad (`--format auto|csv|eagle|kicad-pos`; `auto` tells them from the first row) and writes the table of parts `ref,val,package,x,y,rot,side,dnp` in millimetres to `--out` (a file that exists is never written over: `PHOENIX-PNP-040`) or to the standard output. The units are mm, mil or inch (in the header as `X (mil)`, in the cell as `0.5in`, in a comment as `## Unit = inch`, in the map or in `import_unit`); angles come out from 0 up to 360; the column of the side is optional; a column `dnp` or `fitted` (or `--dnp R1-R5,C7` with ranges of references) marks the parts that are not to be mounted; `import_bottom_mirror_width_mm` mirrors the X of the bottom side and `bottom_rotation` turns its angle. The three readers are the program's own (no other repository has a reader that can be used without adding a dependency) and the layout of the EAGLE text has not been compared with a file written by a real installation. Every error has a code from `PHOENIX-PNP-053` to `PHOENIX-PNP-056` (a line that is not valid, a header that lacks a column of the map, a format that is not known, a range of references that is not valid) and the number of the line.
* **Bill of materials** (`src/bom.rs`, `src/cmd_tools.rs`): `phoenix-pnp bom-check PARTS BOM` reads a BOM (`examples/bom.csv`: the references in a cell, as a range or one for each row, and the value, package, quantity and part number when it has them) and says which references only one side has, which are listed twice, which values or packages differ and which quantities do not add up; a part marked as not to be mounted may be in the BOM or not. An error of the file has the code `PHOENIX-PNP-057` and its line.
* **Stock for a run** (`src/stock.rs`): `phoenix-pnp stock PARTS FEEDERS --boards N` adds the boards up and says, kind of part by kind, how many parts N boards need against the parts in the feeders that are on, which kinds fall short, by how much and after how many boards the first reel runs out; it exits with 1 when something falls short and writes the table with `--out`. `bom-order --have FEEDERS` sets what the feeders already hold against the list of parts to buy and gives the columns `on_hand` and `to_buy`.
* **Camera calibration** (`src/calibrate.rs`): `phoenix-pnp camera-calibrate FILE` fits the model of the camera (size of a pixel, turn, mirror, and where the axis of the nozzle is in the picture) to sightings of a mark at known places of the head (`px,py,x_mm,y_mm` [see `examples/camera_points.csv`], at least three) and prints how well they fit and the `camera_*` lines for the configuration; the lens is not fitted. Its error is the code `PHOENIX-PNP-058`.
* **Nozzle runout** (`src/calibrate.rs`, `src/export.rs`): `phoenix-pnp nozzle-calibrate FILE` fits a circle to readings of the nozzle tip at several angles (`angle_deg,x_mm,y_mm` [see `examples/nozzle_readings.csv`]) and prints `nozzle_runout_mm` and `nozzle_runout_deg`; with the runout above 0 the G-code of `schedule --gcode` moves each place by the opposite of the runout at the angle of the part.
* **Panel of a grid of boards** (`src/panel.rs`, `src/cmd_tools.rs`): `phoenix-pnp panelize BOARD --cols N --rows N --pitch-x MM --pitch-y MM` writes the panel file for a grid of copies of one board, with the same turn for each, three fiducials at a margin (`--fiducials MM`, around the real size of the board given by `--board-size W,H`) and boards marked as bad by their names `P1`, `P2`, ... (`--x-out P2,P5`); a file that exists is never written over.
* **Sheet of the parts with a direction** (`src/polarity.rs`): `phoenix-pnp polarity-sheet PARTS` lists the parts whose reference matches `polarity_refs` or whose package matches `polarity_packages` (diodes, LEDs, chips, transistors, polarized capacitors and connectors by default) with their place and turn and an empty column to tick, for the look after a job; `--side` keeps one side.
* **Slack of the axes** (`src/sim.rs`, `src/motion.rs`): `backlash_x_mm`, `backlash_y_mm` and `backlash_mode` make the simulated machine end a move that reverses an axis short of the place by the slack (`off`, counted by `simulate`) or go past the place and come back from the positive side (`approach`, which takes more time and leaves no error); with a slack of 0 nothing changes.
* **List of parts to buy** (`src/bom.rs`): `phoenix-pnp bom-order BOM --boards N` adds the rows of a BOM that are the same part and gives each line the parts for N boards plus a spare (`order_spare_percent` per cent or `order_spare_min`, whichever is larger; `--spare-percent` and `--spare-min` win), leaving out the parts that `--parts PARTS` marks as not to be mounted; it writes a CSV sorted by package and value.
* **Teaching feeders** (`src/calibrate.rs`): `phoenix-pnp feeder-teach strip` gives the pitch and the angle of a strip from the places of its first and last parts, and `feeder-teach tray` the two pitches and the angle of a tray from three of its cells, with how far the columns are from being square; both print the columns of the file of feeders that they set. `feeder-teach --measures FILE` reads the places of several feeders from one file and prints one line and one row of the feeders file for each (`examples/feeder_measures.csv`).
* **The simulator and the job** (`src/sim.rs`, `src/job.rs`, `src/preflight.rs`, `src/model.rs`): the checks before a job, a machine with a virtual clock and failures that can be injected, and a job that can be paused, taken step by step, saved and resumed.
* **The window** (`src/gui.rs`, `src/gui/`): what the window does besides drawing needs no screen (the files that were loaded, the plan and its problems with fixes, the simulated job, the geometry of the map, the tables with a shared selection, the guide of five steps and the words in eight languages) and is tested; only `src/gui/window.rs`, built with `--features gui`, draws it with egui. The speed of the simulation, the theme and the size drawn for a part of an unknown package are keys of `machine.cfg` (`gui_steps_per_s`, `gui_theme`, `gui_part_size_mm`).
* **Look at the part on the nozzle** (`src/inspect.rs`, `src/cmd_vis.rs`): `phoenix-pnp part-inspect READINGS --sizes SIZES` (or `--packages PACKAGES`) compares what the camera measured of each part (its two sides, where it hangs from the axis and how much it is turned) with the size of its package and says why it is refused (`part_size_tol_mm`, `part_angle_tol_deg`, `part_offset_max_mm`); the size comes from a table of sizes or from the columns `body_long_mm` and `body_short_mm` of the table of packages (`examples/packages_bodies.csv`), and the command exits with 1 while a part is refused or has no size.
* **Tubes and bulk feeders** (`src/feeding.rs`, `src/feedlib.rs`): besides a tape, a tray and a strip, the column `kind` of the table of feeders takes `tube` (a stick or rail whose parts slide to a stop) and `bulk` (loose parts in a bin or a bowl, which come in no fixed direction); both pick from the one place `x`, `y`, as a tape does, take the angle of the parts from `rotation`, are emptied by the count and are checked by `feeder-check`; the Feeders tab of the window offers all five kinds in the eight languages (`examples/feeders_tubes.csv`).
* **Parts placed and time by feeder** (`src/job.rs`, `src/feedrep.rs`, `src/report.rs`): the job notes which feeder each nozzle took its part from (also when another feeder of the kind took over) and counts the part as placed from that feeder, and adds the time of the steps that picked from each feeder (the way to it, the feed, the pick and the look); the work report, its JSON and `simulate --report-feeders` (the columns `placed` and `busy_s` at the end) show them, and a saved job keeps them.
* **Life of the nozzle tips** (`src/nozzlelife.rs`, `src/cmd_tools.rs`, `src/commands.rs`): `simulate --nozzle-life FILE` adds what a job that ran to its end did with each tip (the parts it placed and the times it was mounted) to a small CSV that the person keeps beside the configuration; a job is counted once, a file that is not a record of the tips is never written over, the file it replaces is kept as `FILE.prev` and the counts go down only when `nozzle-life FILE --reset TIP` records that a tip was replaced. `nozzle_life_warn` (default 500000, 0 does not warn) is the number of parts at which a tip is told to the person to look at, by `simulate` and by `nozzle-life`. New error code `PHOENIX-PNP-059`.
* **Which order is fastest** (`src/compare.rs`, `src/commands.rs`): `phoenix-pnp order-compare PARTS FEEDERS` plans the same board with every `order` of the places (nearest, part, height_part, pick, tips, none) and every `tip_strategy` (minimize, as_planned, optimize), and prints them side by side with the batches, the changes of tip, the travel of the head and the seconds, the fastest of each list marked, the one the configuration has marked, and a line that says how many seconds the better setting would save; it takes the options of `schedule` and changes nothing.
* **A picture of the plan** (`src/preview.rs`, `src/commands.rs`): `phoenix-pnp schedule PARTS FEEDERS OUT.json --svg OUT.svg` also writes the plan as one SVG file to look at before the machine moves, to print or to keep with the job: the feeders, the place of every part with the number of its turn and the colour of its nozzle, the line each part follows from its feeder, the dashed way the head travels over the whole job and the totals; it has no script and nothing from outside, whatever is typed in a reference or an id is escaped, and a picture that exists is never written over.
* **Camera calibration from a pattern** (`src/pattern.rs`, `src/cmd_tools.rs`): `phoenix-pnp camera-pattern FILE [--pitch MM]` reads the points of a grid (the inner corners of a chequerboard or the centres of dots) as `col,row,px,py` (`examples/camera_pattern.csv`) and fits the size of a pixel (`camera_units_per_px`) and the radial bending of the lens (`camera_lens_k1`) together with the turn, the shift and the mirror of the sheet in the picture, then says how far the points are from the model with and without the bending and which point is farthest; the lines for the configuration are printed, and the exit code is 1 when a point is over `camera_pattern_max_residual_mm`. The points are `camera_pattern_pitch_mm` apart. The turn and the offset of the camera still come from `camera-calibrate`, which moves the head. The Tools tab has the same calibration, and it refuses to put in the machine a fit that is over the limit.
* **Which fiducial is off** (`src/fidcheck.rs`, `src/gui/workspace.rs`, `src/gui/tables.rs`): when the measured marks of a panel are used, the window works out the residual of every mark (also when the fit is refused, so that the person can see why), the only mark that, left out, brings all the others within `fiducial_max_residual_mm`, and the turn, the scales and the shear against their limits. The table of fiducials marks a mark over the limit with `!` and the one that does not agree with `?` and the panel lists the warnings in words (also in the log); `fidcheck::analyze_marks`, `fidcheck::warnings` and `fidcheck::residual_table` give the same to the command `fiducial-check` and to the Fiducial check tool.
* **Queue of jobs** (`src/queue.rs`, `src/cmd_queue.rs`): several boards or panels one after the other. `phoenix-pnp queue-add QUEUE NAME SOURCE FEEDERS [--panel] [--boards N]` puts a job in a small CSV that survives the program stopping (it is written to a temporary file and moved into place, the file it replaces is kept as `PATH.prev`, and the last line holds a checksum, so a file that was cut or changed by hand is refused); `queue-status [--check]` lists the jobs and what became of each (pending, running, done, failed, skipped) and with `--check` plans the ones still to be made; `queue-edit` skips, resets, revokes the confirmation of or takes out a job; `queue-run` makes the jobs one board at a time on the simulated machine and writes the queue after every board. Nothing runs without a confirmation: with `queue_confirm` on (the default) a job runs only after `queue-confirm` recorded the identity of its plan (the plan and the text of the files it was made from), and a change to a file voids it; `queue_stop_on_failure` (default on) stops the queue at the first job that fails. A second `queue-run` on the same queue is refused while a `.lock` file is there. The Queue tool of the Tools tab does all of this with buttons. New error code `PHOENIX-PNP-060`.
* **Text in UTF-16** (`src/parts.rs`): every table and settings file that the commands and the window read (positions, feeders, packages, BOMs, panels, measured marks) may be UTF-16 with a byte order mark, as Windows programs and spreadsheets save "Unicode text"; it is read like UTF-8. Text that is neither is read as Latin-1 with a note, and a tool of the Tools tab that reads a file refuses it.
* **Correcting the lens** (`src/camera.rs`, `src/sim.rs`): with `camera_correct_lens = on` the camera takes the bending of the lens (`camera_lens_k1`, as `camera-pattern` finds it) out of what it sees, so the readings of the align step are those of a lens that bends nothing; `CameraModel::to_machine_corrected` and `read_corrected` do it by inverting `d * (1 + k1 * d * d)`. Off (the default) leaves the bending in the readings. `camera_units_per_px` should then be the true size of a pixel, as `camera-pattern` gives it.
* **Queue paths from the queue's folder** (`src/queue.rs`, `src/cmd_queue.rs`): with `queue_relative_paths = on` (or `queue-add --relative`) the files of a job that are in the folder of the queue file, or below it, are kept as paths from that folder, so the folder can be moved or shared; `queue-status`, `queue-confirm` and `queue-run` take them back from the folder of the queue, whatever folder they run from. Off (the default) keeps absolute paths; both run the same and give the same plan identity.
* **The queue with buttons** (`src/gui/queuetab.rs`, `src/gui/window.rs`): the Queue tool of the Tools tab reads a queue file as a list of jobs and has buttons to add a job (a form checked in the language of the window), check the plans, confirm the plan of the selected job, revoke the confirmation, skip, reset, take out and run. Each button runs the command of the same name as the program beside the window, so the window and the command line plan, confirm and run in one way (same plan identity, same lock). The first press of Run only asks (`queue_gui_ask_run`, default on); the second starts the run on the simulated machine, in the background, with a Stop button.
* **A simulated camera that can be made worse** (`src/vision.rs`): `vision.cfg` adds `sim_blur_px` (a camera out of focus), `sim_salt_pepper` (pixels that turn black or white), `sim_gradient_levels` (an uneven light), `sim_failure_rate` and `sim_failure_kind` (a share of the pictures spoilt: the part not there, the light off or everything saturated); all are off by default, and the noise of a job that does not use them does not move. `vision-test` and `simulate --vision` use them like the noise that was already there.
* **Readings to try the checks** (`src/visionsim.rs`, `src/cmd_vis.rs`): `phoenix-pnp vision-readings PARTS --sizes F` draws the picture that the simulated camera gives of each part of a position file (hanging a little off the axis and turned, within `--spread` and `--turn`; `--wrong-every N` makes every N-th part one of another size) and writes the readings in the file that `part-inspect` reads; `phoenix-pnp vision-marks DESIGN --shift X,Y --turn DEG --scale S` looks at each mark of a panel that lies as said and writes the measured marks that `fiducial-check` reads. Blur, noise and failures come from `--vision`; a picture in which nothing is found gives no reading and is told with its reason; the same `--seed` gives the same readings.
* **A work report to keep** (`src/report.rs`): `simulate --report-html FILE` (one page with no script and nothing from outside), `--report-text`, `--report` (JSON) and `--report-csv` now also say, for each feeder, how many parts were picked, placed, put aside and left out, and, for each nozzle tip, what the job added to its life (`--nozzle-life` gives the record from before the job), the uses before and after and a warning when a tip gets to `nozzle_life_warn`; the times, the findings of the checks and the errors with their codes were there already.
* **A truer time for the plan** (`src/planner.rs`): with `backlash_mode = approach` the machine goes past a place by the slack of an axis and comes back, which takes time; the plan used to leave it out and now counts it leg by leg (only where there is room before the limit), so its seconds are those of the simulated job, and `AdvancedPlan::approach_s`, the report of the plan and its JSON say how many seconds are for the second passes. Tips are still grouped in batches and the feeders of a batch visited nearest first; without the approach nothing changes.
* **Sizes of the common packages** (`src/pkggen.rs`, `src/cmd_tools.rs`): `phoenix-pnp package-gen 0603 SOT-23-5 SOIC-8 QFN-32-5x5 LQFP-48` (or `--from PARTS` for the packages of a position file) writes the rows of a table of packages (`package,height_mm,tape_mm,body_long_mm,body_short_mm`) from the nominal sizes of the chips of the EIA codes, SOT, SOD, SOIC (narrow and wide), TSSOP, QFN, DFN and QFP, with the width of the tape that holds the body; `--packages` of `part-inspect` and the catalog read the result. A name that is not known is an error with its reason, never a guess.
* **Combined camera calibration** (`src/combined.rs`, `src/cmd_tools.rs`, `src/gui/toolbox.rs`): `phoenix-pnp camera-combined FILE` reads `head_x,head_y,col,row,px,py` (`examples/camera_combined.csv`: a sheet fixed to the bed, the head moved to several places) and, with the place of the sheet (`camera_pattern_origin_x_mm`, `camera_pattern_origin_y_mm`) and its pitch, finds in one fit the size of a pixel, the turn, the mirror, the offset of the nozzle, the bending of the lens and how much the sheet is turned on the bed; the camera of `camera-calibrate` and the lens of `camera-pattern` in one go. It is off by default (`camera_combined = off`; the two separate commands do not depend on it), the head must stand at two or more places, and the offset is only as good as the measure of the origin. The Tools tab has the same calibration; putting it in the machine does not turn `camera_correct_lens` on.
* **More panels in the Tools tab** (`src/gui/toolbox.rs`): the use of the nozzle tips (the record that `nozzle-life` keeps, with the tips at or over `nozzle_life_warn` marked), the inspection of the parts on the nozzle against the sizes of the packages that are loaded, the check of the fiducials (the residual of each mark, the mark that does not agree and a warning for a mark over `fiducial_max_residual_mm`), the check of the table of feeders, the comparison of the orders and of the changes of tip (what the fastest would save) and the picture of the plan as an SVG; each writes its result to the output folder under its own name without going over a file that exists. The setup sheet of the tab reads the record of the nozzle tips when one is typed or read, and then says which tip is worn.
* **The setup sheet** (`src/setup.rs`, `src/commands.rs`, `src/gui/toolbox.rs`): `phoenix-pnp setup-sheet PARTS FEEDERS --boards N` gathers on one page, in the eight languages, the feeders to load in the order of their slots (kind, part, tape, nozzle tip, parts held, place), the parts that are missing for N boards, the nozzle tips of the run with their wear against `nozzle_life_warn` (`--life` reads the record of `simulate --nozzle-life`), the calibrations the job depends on and whether each is set (camera, runout of the nozzle, slack of the axes, place of the tip changer, fiducials of a panel) and the warnings (a feeder that is off but holds a part the board needs, a bulk feeder that holds a part that has a direction); `--out` writes the feeders as a CSV and the command exits with 1 while parts are missing. The Tools tab has the same sheet to print as `setup_sheet.txt`.
* **Tools tab** (`src/gui/toolbox.rs`): the slack of the axes, the runout of the nozzle, the camera calibration, the teaching of feeders, the sheet of the parts that have a direction, the check and the order of a bill of materials, the stock for a run and the panel of a grid of boards, each with the library functions of its command, a text that is typed or read from a file, the result on screen and, where it makes a file, a button that writes it to the output folder without going over a file that exists; the camera and the runout can be put in the machine. The values the forms start with are keys of `machine.cfg` (`order_boards`, `panel_cols`, `panel_rows`, `panel_pitch_x_mm`, `panel_pitch_y_mm`, `panel_fiducial_mm`, `panel_board_width_mm`, `panel_board_height_mm`).

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
phoenix-pnp recipe-check FILE                            whether a recipe is valid, with the code and the line
phoenix-pnp import-centroid FILE [--map F] [--out F]     an import of a CAD position file as the table of parts
phoenix-pnp bom-check PARTS BOM [--config F]              whether the bill of materials and the table of parts agree
phoenix-pnp camera-calibrate FILE [--config F]            the settings of the camera from sightings of a mark, with how well they fit
phoenix-pnp nozzle-calibrate FILE [--config F]            the runout of the nozzle tip from readings at several angles
phoenix-pnp panelize BOARD --cols N --rows N --pitch-x MM --pitch-y MM [--out F] a panel file for a grid of copies of one board
phoenix-pnp polarity-sheet PARTS [--side top|bottom] [--out F] the parts that have a direction, as a CSV for the look after a job
phoenix-pnp bom-order BOM [--boards N] [--spare-percent P] [--out F] the parts to buy for N boards, with a spare
phoenix-pnp feeder-teach strip|tray [places and counts] | --measures FILE   the pitch and the angle of a strip or a tray from places measured on the machine
phoenix-pnp stock PARTS FEEDERS [--boards N] [--side top|bottom] [--out F] whether the feeders hold enough parts for N boards, kind by kind (exits with 1 if not)
phoenix-pnp part-inspect READINGS [--sizes F] [--packages F] [--out F] the parts on the nozzle against the size of their packages (exits with 1 if one is refused)
phoenix-pnp fiducial-check DESIGN MEASURED [--config F] the turn, scale, shear and shift that the measured marks put on the panel, against the limits
phoenix-pnp feeder-check FEEDERS [--packages F] [--catalog F] whether the table of feeders agrees with itself (slots, pitches, cells, places that clash)
phoenix-pnp nozzle-life FILE [--warn N] [--reset TIP] the parts each nozzle tip has placed, from a file kept beside the configuration (exits with 1 when a tip is at the warning level)
phoenix-pnp setup-sheet PARTS FEEDERS [--boards N] [--life F] [--out F] what to load in which slot, the parts missing for N boards, the nozzle tips and their wear, the calibrations the job depends on (in the language of --lang; exits with 1 if parts are missing)
phoenix-pnp order-compare PARTS FEEDERS [OPTIONS] the board planned with every order of the places and every way of changing the tips, side by side, the fastest marked
phoenix-pnp camera-pattern FILE [--pitch MM]             the size of a pixel and the bending of the lens from the points of a pattern on a grid
phoenix-pnp queue-add QUEUE NAME SOURCE FEEDERS          puts a job (boards or a panel) at the end of a queue of jobs
phoenix-pnp queue-status QUEUE [--check]                 the jobs of the queue and what became of each; --check plans the ones still to be made
phoenix-pnp queue-confirm QUEUE NAME                     records that the plan of a job, as it is now, may be run
phoenix-pnp queue-edit QUEUE NAME skip|reset|revoke|remove   leaves a job out, starts it again, takes its confirmation back or takes it out
phoenix-pnp queue-run QUEUE [--only NAME]                makes the confirmed jobs one board at a time on the simulated machine
phoenix-pnp camera-combined FILE [--pitch MM] [--origin X,Y]   the camera, the turn of the sheet and the bending of the lens from a pattern fixed to the bed while the head moves (needs camera_combined = on)
phoenix-pnp vision-readings PARTS --sizes F [--seed N]   the readings that the simulated camera gives of the parts, in the file that part-inspect reads
phoenix-pnp vision-marks DESIGN [--shift X,Y] [--turn DEG]   the measured marks of a panel that lies as said, as the simulated camera reads them, for fiducial-check
phoenix-pnp package-gen [NAME...] [--from PARTS]          the rows of a table of packages (body, height, tape) from standard names such as 0603, SOT-23, SOIC-8, QFN-32
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
| `PHOENIX-PNP-044` | The vacuum shows no part on the nozzle after the pick |
| `PHOENIX-PNP-045` | The vacuum shows the part still on the nozzle after the place |
| `PHOENIX-PNP-046` | The machine has no actuator with that name |
| `PHOENIX-PNP-047` | A feeder has too many faults among its last picks: the person decides |
| `PHOENIX-PNP-048` | No head of the machine can reach the part |
| `PHOENIX-PNP-049` | A line of the recipe is not valid |
| `PHOENIX-PNP-050` | The steps of the recipe are not in the order of the job |
| `PHOENIX-PNP-051` | The recipe is larger than its limits allow |
| `PHOENIX-PNP-052` | The recipe was written for a newer version of the program |
| `PHOENIX-PNP-053` | A line of the position file is not valid |
| `PHOENIX-PNP-054` | The column map, or the header it is applied to, does not give the columns that are needed |
| `PHOENIX-PNP-055` | The format of the position file is not known |
| `PHOENIX-PNP-056` | A range of references is not valid |
| `PHOENIX-PNP-057` | A line of the bill of materials is not valid |
| `PHOENIX-PNP-058` | The calibration points are not valid or do not give a result |
| `PHOENIX-PNP-059` | The file of the use of the nozzle tips is not valid |
| `PHOENIX-PNP-060` | The queue of jobs is not valid, or it was cut or changed by hand |

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
cargo run --release -- import-centroid examples/centroid_custom.csv --map examples/centroid.map --out examples/parts_from_centroid.csv
cargo run --release -- import-centroid examples/board.mnt --dnp R1-R2
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

The suite has 889 tests (`cargo test`, run on Windows and Linux in the CI). They check the normal cases, the edge cases and damaged input: empty files, garbage and cut files never panic. Small generated cases (from a fixed seed, so they give the same numbers on every run) check that the plan does not depend on the order of the files, that a walk is never made longer, that a job saved and resumed anywhere ends as the one that never stopped, and that lengths and angles convert back and forth. The CI also runs `cargo fmt --check`, `cargo clippy` with warnings as errors and `python -I tools/ci_validate.py`.

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
│   ├── bom.rs                                   the bill of materials: reading, the check against the board, the order
│   ├── calibrate.rs                             the calibrations of the camera and of the nozzle tip, and the teaching of feeders
│   ├── camera.rs                                the model of a camera: size of a pixel, offsets, turn, mirror and lens
│   ├── catalog.rs                               the tables of parts and packages and the inheritance of their properties
│   ├── cli_args.rs                              the arguments of the command line, strict: an unknown option is an error
│   ├── cmd_queue.rs                             the commands of the queue of jobs: add, status, confirm, edit, run
│   ├── cmd_tools.rs                             the commands that check, calibrate and make files: BOM, calibration, panel
│   ├── cmd_vis.rs                               the commands part-inspect, fiducial-check and feeder-check
│   ├── combined.rs                              the calibration of the camera from a fixed pattern while the head moves
│   ├── commands.rs                              the commands that use the configuration, tables-check and check-bottom
│   ├── compare.rs                               the board planned with every order and tip strategy, side by side
│   ├── config.rs                                the configuration file of the machine: keys, ranges and safety limits
│   ├── error.rs                                 the error codes PHOENIX-PNP-NNN and their meaning in eight languages
│   ├── export.rs                                the report, the list of what is missing and the G-code of the movements
│   ├── feeders.rs                               a part of the logic
│   ├── feeding.rs                               feeders with priority, trays and strips, and the choice of the feeder for a part
│   ├── feedlib.rs                               whether the library of feeders agrees with itself
│   ├── feedrep.rs                               the picks of each feeder in a job
│   ├── fidcheck.rs                              what the fiducial fit says: turn, scale, shear, shift, the mark that disagrees
│   ├── fiducial.rs                              fiducials of a panel: the fit of the measured marks and the correction of the parts
│   ├── gui.rs                                   the window: modules that need no screen, and the drawing in window.rs
│   ├── gui/app.rs                               the state of the window: load, plan, start, pause, step, fix, save
│   ├── gui/browse.rs                            the file picker: folders and files, up, typed paths
│   ├── gui/jobtab.rs                            the Job tab without the window: states, filters, progress, error dialog
│   ├── gui/log.rs                               the log with levels, and the text of the diagnostic that can be copied
│   ├── gui/problems.rs                          the problems of a job and the fixes that put them right
│   ├── gui/queuetab.rs                          the Queue tool without the window: the list of jobs, the form and the buttons
│   ├── gui/samples.rs                           samples, from a small board to a large one
│   ├── gui/session.rs                           the simulated job that is started, paused, resumed and stepped
│   ├── gui/settings.rs                          what the window remembers: language, theme, layers, folders and the width of the columns
│   ├── gui/shot.rs                              the picture mode of the window (--shot): its arguments
│   ├── gui/tables.rs                            the rows, the sorting and the selection shared by the tables
│   ├── gui/texts.rs                             every word of the window in the eight languages
│   ├── gui/theme.rs                             the colours: phoenix green, light and high contrast
│   ├── gui/toolbox.rs                           the Tools tab without the window: the tools, their forms and what they write
│   ├── gui/view.rs                              the geometry of the map: bodies, polarity marks, fit, zoom and hit
│   ├── gui/window.rs                            the drawing with egui, built with the gui feature: tabs, map and tables
│   ├── gui/wizard.rs                            the guide of five steps from nothing to a simulated job
│   ├── gui/workspace.rs                         the files that were loaded, the plan and the edits, without a window
│   ├── gui_main.rs                              the window as a program of its own, without a console on Windows
│   ├── heads.rs                                 several heads: reach, sharing out the parts, waits for a feeder
│   ├── import.rs                                the import of position files: a centroid CSV with a column map, EAGLE text, KiCad .pos
│   ├── inspect.rs                               the look at the part on the nozzle: size, turn and place
│   ├── job.rs                                   the job: steps, pause, retries, policies, statistics and saved state
│   ├── lib.rs                                   the library: what every command uses
│   ├── loadsheet.rs                             the load sheet: the slot of each kind of tape
│   ├── main.rs                                  the command line, and the entry of the window when it runs with no command
│   ├── marks.rs                                 marks of two levels: the parts that are marks, the fit of a panel and of each board
│   ├── model.rs                                 the parts of a machine as traits: machine, nozzle, feeder, actuator, camera, driver
│   ├── motion.rs                                the time a move takes, with acceleration and jerk
│   ├── nozzlelife.rs                            the use of the nozzle tips: parts placed, mountings, replacements, the warning level
│   ├── panel.rs                                 panels: boards and panels inside panels, each in its place
│   ├── parts.rs                                 the position file and the parts that are not to be mounted
│   ├── pattern.rs                               the calibration of the camera from a pattern: pixel size, lens bending, residual
│   ├── pkggen.rs                                the sizes of the common packages from their names
│   ├── plan.rs                                  a part of the logic
│   ├── planner.rs                               the planner that knows the machine: ranks, order, tips and their search, angles and time
│   ├── polarity.rs                              the sheet of the parts that have a direction
│   ├── preflight.rs                             the checks before a job, with a severity and what they point at
│   ├── preview.rs                               a picture of the plan: feeders, places, the way of the head
│   ├── queue.rs                                 the queue of jobs: the file with its checksum, states, the gate of the confirmation
│   ├── recipe.rs                                the recipe of a job: its file, its steps and the parameters by package, feeder and tip
│   ├── report.rs                                the report of a job: phases, picks, groups, errors; text, HTML, CSV, JSON
│   ├── rotation.rs                              corrections of the rotation by package, and how a limited nozzle turns
│   ├── setup.rs                                 the setup sheet: feeders by slot, missing parts, tips, calibrations
│   ├── sim.rs                                   the simulated machine: virtual clock and failures that can be injected
│   ├── stock.rs                                 the parts the feeders hold against the parts a run of boards needs
│   ├── tips.rs                                  the nozzle tips that a package accepts and the choice of one for every kind of part
│   ├── units.rs                                 lengths and places that carry their unit
│   ├── vision.rs                                the vision pipeline: filter, threshold, spots and the smallest turned rectangle
│   └── visionsim.rs                             the readings of the simulated camera for part-inspect and fiducial-check
├── tests/
│   ├── align.rs                                 tests of the align step: camera, corrected place, tolerance and time
│   ├── approach.rs                              tests of the time of the plan with the approach: the second passes and the job's seconds
│   ├── backlash.rs                              tests of the slack of the axes of the simulated machine
│   ├── bom.rs                                   tests of the bill of materials: reading, the check, the code, bom-check
│   ├── bom_order.rs                             tests of the list of parts to buy
│   ├── calibrate.rs                             tests of the calibrations: the fits, the files, the lines of the configuration, the G-code
│   ├── camera.rs                                tests of the camera model, the cameras of the heads and the camera of the simulator
│   ├── combined.rs                              tests of the combined calibration: the file, the fit, the mode, the command, the tool
│   ├── commands.rs                              tests of the commands that use the configuration
│   ├── compare.rs                               tests of the comparison of orders: the fastest, the text, the advice, the command
│   ├── encoding.rs                              tests of the encoding of the files: UTF-8, UTF-16 and Latin-1
│   ├── feeding.rs                               tests of the feeders: priority, trays and strips
│   ├── feedlib.rs                               tests of the check of the library of feeders
│   ├── feedrep.rs                               tests of the picks of each feeder
│   ├── fidcheck.rs                              tests of the check of the fiducial fit
│   ├── gui.rs                                   tests of the window without a screen: state, plan, job, map, tables and guide
│   ├── heads.rs                                 tests of the heads: lines of the configuration, reach, sharing out, the clock and schedule
│   ├── import.rs                                tests of the import: formats, units, bottom side, DNP, ranges, errors with their line
│   ├── inspect.rs                               tests of the look at the part on the nozzle
│   ├── job.rs                                   tests of the job: steps, retries and saved state
│   ├── jobreport.rs                             tests of the work report to keep: the parts lost by feeder, the life of the tips, the files
│   ├── jobtab.rs                                tests of the Job tab without a screen: states, filters, progress, dialog
│   ├── loadsheet.rs                             tests of the load sheet and the exports
│   ├── marks.rs                                 tests of the boards marked as bad, the parts that are marks and the marks of two levels
│   ├── model.rs                                 tests of the data model: codes, lengths, configuration and tables
│   ├── more.rs                                  more tests of the library and the command line
│   ├── names.rs                                 tests of the names of the files: no two that differ only in case, none that Windows cannot hold
│   ├── nozzlelife.rs                            tests of the use of the nozzle tips: the file, what a job adds, the commands
│   ├── panel.rs                                 tests of panels and of their fiducials
│   ├── panelize.rs                              tests of the panel of a grid of boards and of panelize
│   ├── pattern.rs                               tests of the calibration from a pattern: the file, the fit, the limit, the command and the tool
│   ├── pkggen.rs                                tests of the sizes of the packages: the families, the errors, the table, the command
│   ├── planner.rs                               tests of the time of the moves and of the planner
│   ├── pnp.rs                                   tests of the library and the command line
│   ├── polarity.rs                              tests of the sheet of the parts that have a direction
│   ├── preflight.rs                             tests of the checks before a job
│   ├── preview.rs                               tests of the picture of the plan: its parts, escaping, size and --svg
│   ├── queue.rs                                 tests of the queue of jobs: the file, the checksum, the gate, the commands and the tool
│   ├── readme.rs                                tests that the README can be run and is complete
│   ├── recipe.rs                                tests of the recipe: the file, the limits, the steps, the rules in the job and the saved state
│   ├── report.rs                                tests of the job report: phases, picks, histograms, groups, decisions, formats
│   ├── samples.rs                               tests of the examples of the calibrations and of the file of measures of the feeders
│   ├── setup.rs                                 tests of the setup sheet: lists, languages, command and tool
│   ├── shot.rs                                  tests of the picture mode of the window: its arguments, read and checked without a screen
│   ├── sim.rs                                   tests of the simulated machine
│   ├── stock.rs                                 tests of the stock for a run and of the parts to buy
│   ├── strict.rs                                tests of the command line: options and words it does not know
│   ├── tables.rs                                tests of the parts not to be mounted, of tables-check and of the report of a job as a CSV
│   ├── teach.rs                                 tests of the teaching of strips and trays
│   ├── tips.rs                                  tests of the tips that a package accepts: the rules, the choice and the plan
│   ├── toolbox.rs                               tests of the Tools tab: its settings, each tool on the examples, what it applies and writes
│   ├── vacuum.rs                                tests of the vacuum, the actuators and the window of faults of a feeder
│   ├── vis_cli.rs                               tests of the commands part-inspect, fiducial-check, feeder-check and --report-feeders
│   ├── vision.rs                                tests of the vision pipeline: pictures checked by hand, failures, settings
│   ├── vision_job.rs                            tests of the vision in the job and vision-test: readings, failures, saved state
│   └── visionsim.rs                             tests of the worse camera and of the readings it gives to the two checks
├── examples/
│   ├── board.mnt                                the positions of a board as the text of EAGLE has them
│   ├── board.pos                                a sample to try the program with
│   ├── board_dnp.csv                            a board with a dnp column: two of its parts are not to be mounted
│   ├── bom.csv                                  a bill of materials that agrees with board.pos
│   ├── camera_combined.csv                      sightings of a fixed pattern from four places of the head for camera-combined
│   ├── camera_points.csv                        sightings of a mark for camera-calibrate
│   ├── camera_pattern.csv                       points of a pattern on a grid for camera-pattern
│   ├── catalog.csv                              a table of parts: rank and height
│   ├── centroid.csv                             a centroid CSV with a column of parts not to be mounted and a value with a comma
│   ├── centroid.map                             the map of the columns of centroid_custom.csv
│   ├── centroid_custom.csv                      a centroid CSV with other column names, mils in the headers and words for the sides
│   ├── feeder_measures.csv                      places measured for a strip and a tray, for feeder-teach
│   ├── feeders.csv                              a sample to try the program with
│   ├── feeders_kinds.csv                        feeders of every kind: tape, strip, tray and one switched off
│   ├── feeders_tubes.csv                        a table of feeders with a tube and a bulk feeder besides a tape
│   ├── fid_design.csv                           the designed places of four fiducials
│   ├── fid_measured.csv                         the same four fiducials as measured on the machine
│   ├── job.recipe                               a recipe with every step and rules by package, feeder and tip
│   ├── job_basic.recipe                         the shortest recipe: prepare, pick and place
│   ├── machine.cfg                              the settings of the machine, every key explained
│   ├── marks.csv                                the measured places of the fiducials of the panel
│   ├── nozzle_life.csv                          the use of two nozzle tips, one of them over the level of the warning
│   ├── nozzle_readings.csv                      readings of the nozzle tip at several angles for nozzle-calibrate
│   ├── nozzle_reads.csv                         readings of parts on the nozzle for part-inspect
│   ├── package_sizes.csv                        the size of the body of each package
│   ├── packages.csv                             a table of packages: height, nozzle tip, rank and tape
│   ├── packages_bodies.csv                      a table of packages with the size of the body of each, for part-inspect
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

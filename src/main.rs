// EH3D-PHOENIX-PNP - command line.
// Copyright (C) 2026 JuanenRac (Electro Hobby 3D). GPL-3.0-or-later.
#![forbid(unsafe_code)]

mod cli_args;
mod cmd_drive;
mod cmd_fab;
mod cmd_flow;
mod cmd_import;
mod cmd_more;
mod cmd_nodes;
mod cmd_queue;
mod cmd_server;
mod cmd_shop;
mod cmd_tools;
mod cmd_vis;
mod commands;

use cli_args::{finish, flag, num, switch};
use eh3d_phoenix_pnp::config::MachineConfig;
use eh3d_phoenix_pnp::feeders::{assign, assign_side, from_csv as feeders_csv};
use eh3d_phoenix_pnp::parts::{parse, Side};
use eh3d_phoenix_pnp::plan::{plan, Settings};
use std::env;
use std::fs;
use std::process::ExitCode;

const HELP: &str = "phoenix-pnp: pick and place assembly

  phoenix-pnp [gui [FILES...]]                 the window, with the files loaded (built with --features gui; run with no command it opens too)
  phoenix-pnp gui [FILES...] --shot OUT.png [--size WxH] [--theme phoenix|light|contrast] [--wait N] [--lang CODE] [--window TAB] [--steps N] [--open FILE] [--replace]   a picture of the window with the files loaded, on the tab TAB (job, parts, packages, feeders, panels, boards, loadsheet, machine, problems or log) after N steps of the simulated job (built with --features gui)
  phoenix-pnp parts PARTS                      the parts of a position file (KiCad .pos or CSV) in millimetres
  phoenix-pnp check PARTS FEEDERS              which parts have no feeder and which feeders will run out
  phoenix-pnp plan PARTS FEEDERS OUT.json [--nozzles 2] [--side top|bottom] [--board-width MM] [--origin-x MM] [--speed MM_S] [--partial]

  phoenix-pnp schedule PARTS FEEDERS OUT.json [OPTIONS] [--report-text F] [--gcode F] [--missing F] [--svg F] [--partial]
  phoenix-pnp preflight PARTS FEEDERS [OPTIONS]            the checks before a job, without writing anything
  phoenix-pnp simulate PARTS FEEDERS [OPTIONS] [--state F] [--steps N] [--report F] [--report-csv F] [--report-text F] [--report-html F] [--report-feeders F] [--nozzle-life F] [--vision F] [--recipe F] [--place-offsets F] [--hooks F] [--hooks-dry-run] [--journal F] [--partial]
  phoenix-pnp loadsheet PARTS SLOTS OUT.csv [OPTIONS] [--feeders-out F]
  phoenix-pnp setup-sheet PARTS FEEDERS [OPTIONS] [--boards N] [--life F] [--out F]   what to load in which slot, the parts missing for N boards, the nozzle tips and how worn they are, the calibrations the job depends on and whether they are set, in the language of --lang; exits with 1 when parts are missing
  phoenix-pnp order-compare PARTS FEEDERS [OPTIONS]   the board planned with every order of the places and every way of changing the nozzle tips, side by side: batches, tip changes, travel and seconds, the fastest marked and what it would save over the configuration
  phoenix-pnp config-check FILE [--lang CODE]
  phoenix-pnp check-bottom PARTS [--config FILE] [--lang CODE]   the angle each rule of the bottom side (keep, mirror, half_turn) gives to the parts of the bottom side
  phoenix-pnp tables-check [--parts F] [--packages F] [--catalog F] [--feeders F] [--slots F] [--config F] [--lang CODE]   whether each table is valid, with the code and the row of an error
  phoenix-pnp recipe-check FILE [--config F] [--lang CODE]   whether a recipe is valid, with the code and the line of an error
  phoenix-pnp bom-check PARTS BOM [--config F] [--lang CODE]   whether the bill of materials and the table of parts agree (references, values, packages, quantities)
  phoenix-pnp camera-calibrate FILE [--config F] [--lang CODE]   the settings of the camera from sightings of a mark (px,py,x_mm,y_mm), with how well they fit
  phoenix-pnp camera-pattern FILE [--pitch MM] [--config F] [--lang CODE]   the size of a pixel and the bending of the lens from the points of a pattern on a grid (col,row,px,py), with how well they fit; exits with 1 when a point is over camera_pattern_max_residual_mm
  phoenix-pnp camera-combined FILE [--pitch MM] [--origin X,Y | --touch] [--config F] [--lang CODE]   the camera, the turn of the sheet and the bending of the lens from a pattern fixed to the bed while the head moves (head_x,head_y,col,row,px,py); off until camera_combined is on; --touch (or camera_combined_touch = on) takes the origin from a row origin,X,Y of the file; exits with 1 when a sighting is over camera_pattern_max_residual_mm
  phoenix-pnp package-gen [NAME...] [--from PARTS] [--out F] [--config F] [--lang CODE]   the rows of a table of packages (height, tape, body) for packages named by their standard (0603, R0805, SOT-23, SOIC-8, TSSOP-14, QFN-32-5x5, LQFP-48...) and for those of a position file; exits with 1 when a name is not known
  phoenix-pnp package-kicad PATH... [--height MM] [--out F] [--config F] [--lang CODE]   the rows of a table of packages (body, tape, height) from footprint files of KiCad (.kicad_mod, or a folder of them): the body is the drawing on the fabrication layer; the height is that of the package when its name is a known one, or --height; exits with 1 when a footprint is not understood
  phoenix-pnp place-learn READINGS --state F [--residual] [--dry-run] [--config F] [--lang CODE]   learns the mean offset of the placement of each package from readings (package,dx_mm,dy_mm), leaving the strays out and holding it within place_learn_cap_mm; off until place_learn is on; --show and --forget PACKAGE read and trim the file
  phoenix-pnp job-export PARTS FEEDERS OUT [OPTIONS] [--state F] [--partial]   the plan and where the job is (the saved state of simulate, if given) in one JSON file for other programs
  phoenix-pnp aside-sheet PARTS FEEDERS --state F [--pending] [--out F] [OPTIONS]   the parts that a person has to place by hand after the job of a saved state: the ones put aside and left out (and with --pending the ones not placed yet), sorted by value and package, with why; exits with 1 when there are any
  phoenix-pnp feeder-slots PARTS FEEDERS [--max-plans N] [--out F] [OPTIONS]   looks for the places of the feeders that make the plan fastest, by swapping the places of two feeders of the same kind (tape, tube or bulk) while that saves time (at most N plans, 5000 by default); --out writes the table of feeders with the new places and slot names; exits with 1 when a better way to stand them exists
  phoenix-pnp simulate-runs PARTS FEEDERS [--runs N] [--seed S] [--out F] [OPTIONS] [--partial]   runs the job N times (20 by default) on the simulated machine, each time from another seed (S, S+1, ...; the sim_seed of the settings by default), and says how many runs placed every part, how many an error stopped (and which), the parts put aside and the times; --out writes a row for each run; exits with 1 when a run did not place every part
  phoenix-pnp reel-runway PARTS FEEDERS [--boards N] [--out F] [OPTIONS] [--partial]   how many whole boards each reel of the feeders that are switched on lasts, with the parts left over after them, the reel that runs out first marked; --out writes the table; exits with 1 when a reel lasts fewer than N boards (1 by default)
  phoenix-pnp nozzle-compare PARTS FEEDERS [--max N] [--out F] [OPTIONS]   the board planned with one nozzle on the head, then two, up to N (6 by default), side by side: batches, tip changes, travel and seconds, the fastest marked and what each nozzle takes off the one before; the heads of the settings are left out; --out writes the table; exits with 1 when no number of nozzles gives a plan
  phoenix-pnp simulate-hotspots PARTS FEEDERS [--runs N] [--seed S] [--out F] [OPTIONS] [--partial]   runs the job N times on the simulated machine (simulate_runs_count of the settings by default), each time from another seed, and says for each feeder the picks that failed, the runs in which it was switched off and the parts put aside, the worst first; --out writes a row for each feeder; exits with 1 when a feeder gave trouble
  phoenix-pnp queue-changeover QUEUE [--out F] [--config F] [--lang CODE]   what has to be changed on the feeders between the jobs of the queue that are still to be made: the reels to change or put in, the ones that may stay and how many stay as they are; --out writes a row for each feeder; exits with 1 when a reel has to be changed
  phoenix-pnp camera-selftest [--points N] [--noise PX] [--field PX] [--seed S] [--tol MM] [--out F] [--config F] [--lang CODE]   makes up the camera of the settings, looks at a mark in places chosen by chance from the seed, fits the camera as camera-calibrate does and says how far the one found is from the one made up; --out writes the sightings for camera-calibrate; exits with 1 when the error is over MM (0.05 by default)
  phoenix-pnp machine-template [NAME] [--out F]   with no name, the starting points for the settings of a machine (desktop-1, desktop-2, dual-head, production-4, simulation, server, full); with one, its settings, printed or written to --out
  phoenix-pnp lot-trace PARTS FEEDERS LOTS [--serial S] [--out F] [--config F] [--lang CODE]   the lot of the reel each placed part came from, from a table feeder,lot[,reel][,date_code][,supplier] of what is in the feeders; --out writes a row for each part, with --serial as the board; exits with 1 when a part has no feeder or a feeder that gives parts has no lot
  phoenix-pnp export-assembler PARTS [--format cpl|kicad-pos|generic] [--side all|top|bottom] [--unit mm|inch] [--out F] [--config F] [--lang CODE]   the parts in the shape that an assembler or another program asks for
  phoenix-pnp split-job PARTS FEEDERS --machines N [--out F] [--config F] [--lang CODE]   whole feeders given to N machines so that each has about the same work (the time of a pick and a placement); --out writes a row for each part; exits with 1 when a part has no feeder
  phoenix-pnp failure-odds PARTS FEEDERS [--rules F] [--min-ok PERCENT] [--out F] [--config F] [--lang CODE]   the chance that a board goes through with no part in error, from the chances of failure and the retries of the settings (by package, of a file of rules pattern,feed_retries,pick_retries,place_retries); exits with 1 when it is under PERCENT (0 by default: never)
  phoenix-pnp pos-transform PARTS [--rotate DEG] [--flip] [--shift X,Y] [--zero] [--format cpl|kicad-pos|generic] [--unit mm|inch] [--out F] [--config F] [--lang CODE]   the whole table turned counter-clockwise about the origin, flipped left to right (the sides swap), moved, and with --zero moved so that its lowest left corner is the origin
  phoenix-pnp nozzle-zcal [FILE | --simulate] [--nozzles N] [--probes N] [--noise MM] [--spread MM] [--seed S] [--tol MM] [--out F] [--readings-out F] [--config F] [--lang CODE]   the height of every nozzle tip from probings of the bed (nozzle,z_mm) or from made-up ones: its mean, how much the probings differ, its offset from the mean of the tips and its correction, and the tips that are out of line, noisy or probed once; exits with 1 when a tip needs a look
  phoenix-pnp panel-fids PANEL [--x-out P2,P5] [--min-sep MM] [--out F] [--config F] [--lang CODE]   the marks of every board of a panel in the coordinates of the panel, and the boards that cannot be aligned by their own marks (none, one, or the farthest two too close); exits with 1 when a board cannot
  phoenix-pnp tape-steps PARTS FEEDERS [--out F] [OPTIONS] [--partial]   the tapes checked at every step of the plan: which part of the reel each pick is, the parts left, the pick place, the advance of a strip against its pitch, and the steps that are low, last, empty, off pitch or out of reach; exits with 1 when a step is a fault
  phoenix-pnp shift-oee PARTS FEEDERS [--runs N] [--seed S] [--shift MINUTES] [--reel-change SECONDS] [--min-oee PERCENT] [--out F] [OPTIONS] [--partial]   the effectiveness of a shift on the simulated machine: availability (less the changes of reels), performance (ideal time against the runs) and quality (runs that placed every part), and the boards that fit; exits with 1 when under PERCENT
  phoenix-pnp import-assembler FILE [--format cpl|kicad-pos|generic] [--unit mm|inch] [--out F] [--config F] [--lang CODE]   reads a position file of an assembler or of another program (Mid X, Center-X(mm), Top, TopLayer, 12.5mm, 490mil) and gives the parts in millimetres in a shape every command reads
  phoenix-pnp bom-merge FILE[:BOARDS]... [--max-rows N] [--out F] [--config F] [--lang CODE]   the bills of materials of several boards added up into one list to buy, each for the number of boards wanted, with the parts that are the same joined and the bill each is for
  phoenix-pnp job-fingerprint FILE... [--config F] [--lang CODE]   the SHA-256 of each table, read with the line endings and the spaces at the end of a line made alike (the same table saved on Windows and on Linux has the same fingerprint), and one for the set
  phoenix-pnp serial-sign PARTS FEEDERS --serial S [--lots F] [--prev SIG] [--key-file F] [--out F] [--partial] [--config F] [--lang CODE]   the record of one board by its serial: the fingerprints of its tables, each placed reference with its feeder and lot, and a signature (SHA-256, or HMAC-SHA-256 with the key of a file); --prev chains the records of a run
  phoenix-pnp serial-verify FILE [--key-file F] [--prev SIG] [--config F] [--lang CODE]   reads a record that serial-sign wrote and says whether it is whole (the shape, the counts and the signature); exits with 1 when it is not
  phoenix-pnp shift-plan PARTS FEEDERS [--boards N] [--shift MINUTES] [--reel-change SECONDS] [--out F] [OPTIONS] [--partial]   the shift board by board: at which board and which minute a reel has to be changed, how many boards the shift holds, the time the changes take and the boards an hour
  phoenix-pnp head-split PARTS FEEDERS [--heads N] [--out F] [--config F] [--lang CODE]   a machine with N heads (up to 8) on the same feeders: the board cut into bands against whole feeders to a head, how long each head works and waits, and when the board is done
  phoenix-pnp footprint-check PARTS [--packages F] [--catalog F] [--out F] [--config F] [--lang CODE]   how each package of the board stands against the table of packages: known, an alias of a row, made from its name, a mistake away from a row, or unknown; exits with 1 when one needs a look
  phoenix-pnp reject-report PARTS FEEDERS REJECTS [--boards N] [--max-rate PERCENT] [--out F] [--config F] [--lang CODE]   the rejected parts of a run (reference,reason[,board]) by reason in the order of a Pareto chart, by package, by feeder and by reference; exits with 1 when a reference is rejected on --max-rate per cent of the boards or more
  phoenix-pnp retry-policy PARTS FEEDERS RULES [--out F] [--config F] [--lang CODE]   the retries that rules on a reference, a value, a package or a feeder give each part (the first that fits wins) and the chance of a good board; exits with 1 when a rule fits no part or never acts
  phoenix-pnp clearance PARTS --packages F [--catalog F] [--min MM] [--out F] [--config F] [--lang CODE]   the pairs of parts on one side whose bodies are nearer than --min (clearance_min_mm) or overlap; exits with 1 when there is one
  phoenix-pnp bom-cost BOM PRICES [--boards N] [--spare PERCENT] [--currency TEXT] [--max-rows N] [--out F] [--config F] [--lang CODE]   what the run costs from a table of prices with price breaks (part_number|value[,package],price[,qty_from]); exits with 1 when a part has no price
  phoenix-pnp lot-age PARTS FEEDERS LOTS --week YYWW [--max-weeks N] [--out F] [--config F] [--lang CODE]   how old the lot in each feeder is, from its date code, against the week given and lot_max_weeks; exits with 1 when a lot is old or has no readable code
  phoenix-pnp export-fab PARTS [--bom BOM] [--side all|top|bottom] [--mount] [--dnp-mode column|omit] [--cpl-out F] [--bom-out F] [--config F] [--lang CODE]   the placement list (designator, Mid X, Mid Y, Layer, Rotation; one part on each line, surface and through-hole, the parts not to be mounted marked in the column fab_dnp_column) and the bill of materials (MPN, designators, quantity, description, footprint) that board assembly houses ask for
  phoenix-pnp fab-check FILE [--config F] [--lang CODE]   what an assembly house would refuse in a placement list: a list or a range in a designator cell, a designator twice, a layer that is not Top or Bottom, a place that is not a number; exits with 1 when there is a line
  phoenix-pnp convert-cpl FILE [--from kicad|altium|eagle|jlcpcb|generic] [--to jlcpcb|cpl|kicad-pos|generic] [--origin X,Y] [--flip-y] [--cw] [--rot-offset DEG] [--bottom keep|mirror|half_turn] [--out F] [--config F] [--lang CODE]   a position file of KiCad, Altium, EAGLE or an assembly house brought to another origin, Y direction and way of counting the rotation (the settings cpl_*)
  phoenix-pnp ipc-name NAME... [--density L|N|M] | --make FAMILY [--pitch MM] [--span MM] [--length MM] [--width MM] [--height MM] [--pins N] [--config F] [--lang CODE]   names of footprints by the convention of IPC-7351B: a name taken apart, a chip package of the table (R_0603) in that convention, or a name made from the sizes
  phoenix-pnp footprint-find QUERY... [--packages F] [--catalog F] [--limit N] [--distance N] [--config F] [--lang CODE]   the names of the table of packages that are the query, another name for the same footprint (R_0603, Resistor_SMD:R_0603_1608Metric, RESC1608X55N), hold its words or are a few letters away; exits with 1 when a query finds nothing
  phoenix-pnp driver-check PROFILE [--config F] [--lang CODE]   reads a profile of a G-code driver (Marlin by serial line, Klipper by Moonraker) and says what it holds; every value is checked against its range and nothing is sent anywhere
  phoenix-pnp driver-test PROFILE TRANSCRIPT [--config F] [--lang CODE]   plays recorded answers of a controller (expected | line) against the patterns of a profile and names the lines it reads differently; exits with 1 when there is one
  phoenix-pnp driver-run PARTS FEEDERS PROFILE [--side top|bottom] [--board-width MM] [--fault error@N|silent@N|halt@N|busy@N|offset@MM|slow@F|jitter@MM|strict] [--seed S] [--script-out F] [--log-out F] [--partial] [--config F] [--lang CODE]   plans the board and sends it, as the lines of the profile, to a SIMULATED controller with the timeouts, retries and stop of the profile; no port is opened
  phoenix-pnp safety-replay EVENTS [--profile F] [--config F] [--lang CODE]   replays events (success, retry, timeout, error, mismatch, vacuum, limit, ack) against the rules that only slow the machine down after a trouble; exits with 1 when it ends held or stopped
  phoenix-pnp homing-repeat PROFILE [--runs N] [--seed S] [--jitter MM] [--max-spread MM] [--config F] [--lang CODE]   homes a simulated machine N times and says how far apart the places it lands at are; exits with 1 over the limit
  phoenix-pnp bringup PROFILE [--fault ...] [--seed S] [--config F] [--lang CODE]   the steps of a first start on a simulated controller (connect, home, position, each actuator, a square) with PASS or FAIL for each; exits with 1 on a FAIL
  phoenix-pnp import-openpnp [--machine F] [--packages F] [--parts F] [--out-dir D] [--config F] [--lang CODE]   reads, and never writes to, the configuration files of OpenPnP (the machine with its feeders, the packages, the parts) into the tables of this program with the units in millimetres; every field it cannot map is told with its file and line; exits with 1 when a row could not be read
  phoenix-pnp assist PARTS [--feeders F] [--order reel|reference] [--process pnp|manual|FILE] [--answers F] [--resume F] [--out F] [--config F] [--lang CODE]   the guided manual mode: the parts of the board one after the other (a reel at a time), where each goes and how it is turned; with --process the walk goes phase by phase (load, place a side, oven, solder, inspect: a file of `phase = NAME | task, task` lines or one of the two presets) and the tasks that are not placing wait for `done`; the person types the reference when it is placed (skip, back, quit); writes a checklist; no machine is involved
  phoenix-pnp align-iterate [--error X,Y] [--noise MM] [--scale K] [--seed S] [--blind N] [--max-step MM] [--tolerance MM] [--iterations N] [--gain G] [--config F] [--lang CODE]   looks, moves a part of what it saw and looks again against a camera that is made up, with a cap on every step; exits with 1 when the head does not align
  phoenix-pnp vacuum-calibrate READINGS [--out F] [--config F] [--lang CODE]   the thresholds of the vacuum (vacuum_pick_min_kpa, vacuum_release_max_kpa) from readings state,kpa (free, held, released), a margin inside the gap; exits with 1 when the readings overlap
  phoenix-pnp test-board-gen grid|mixed|dense PARTS_OUT FEEDERS_OUT [--seed S] [--count N] [--pitch MM] [--print] [--config F] [--lang CODE]   a board made up from a seed (the same seed gives the same bytes) with its feeders, to compare machines and settings
  phoenix-pnp dfm-check PARTS FEATURES [--packages F] [--catalog F] [--out F] [--config F] [--lang CODE]   the checks of the board for assembly (pads, spaces, holes in pads, tracks in the middle of a pad, fiducials, tooling holes, tabs) against the settings dfm_*, in the eight languages; exits with 1 on a warning or an error
  phoenix-pnp stencil-apertures FEATURES [--out F] [--config F] [--lang CODE]   the openings of the paste stencil for the pads of a table of features (reduced, the thermal pads split into windows) and the warnings (thickness, reduction, cover, web, area ratio) against the settings stencil_*; exits with 1 on a warning
  phoenix-pnp sim-busy JOURNAL [--out F] [--config F] [--lang CODE]   how busy each nozzle and feeder was in a simulated job and where the machine waited, from the journal of simulate --journal; exits with 1 when a step did not end well
  phoenix-pnp sim-tower JOURNAL [--out F] [--config F] [--lang CODE]   the light a signal tower would show at each second of a simulated job (red on a failure, yellow on a slow step or a wait, green otherwise); exits with 1 when the tower was red
  phoenix-pnp server-discover [--sim] [--config F] [--lang CODE]   looks for servers on the local network (needs server_mdns = on and a build with --features mdns)
  phoenix-pnp server-request PARTS FEEDERS --out DIR [--offset X,Y] [--partial] [--config F] [--lang CODE]   writes the requests that server-send would make, feeders.json and place.json, for other programs
  phoenix-pnp server-send PARTS FEEDERS [--offset X,Y] [--keep-feeders] [--start] [--follow] [--timeout S] [--sim] [--partial] [--dry-run] [--config F] [--lang CODE]   with mode = server, puts the table of feeders on the machine of the EH3D server (unless --keep-feeders), prepares the placements as a job and, with --start or --follow, starts it; --follow reads the job until it is over; --dry-run asks the server to go through the job without taking a part off a feeder; --sim does it all against a server in memory; the parts no feeder holds are told; exits with 1 when the job did not end done
  phoenix-pnp server-status [JOB] [--config F] [--lang CODE]   the machine of the server (its state and the feeders it holds) or, with a job id, how that job goes
  phoenix-pnp server-job ID --start|--pause|--resume|--cancel [--config F] [--lang CODE]   tells a job of the server what to do and says how it stands
  phoenix-pnp server-feeders [list | set SLOT PART COUNT | clear SLOT] [--sim] [--out F] [--config F] [--lang CODE]   the feeders the machine of the server holds with where each is picked from and its calibration, or one slot declared, refilled or emptied at once with no job; exits with 1 when a feeder holds no parts
  phoenix-pnp server-calibrate feeder|nozzle|camera TARGET [--offset X,Y] [--timeout S] [--sim] [--config F] [--lang CODE]   makes a calibration job on the machine of the server (the target of a feeder is its slot, up to 5 mm of offset on each axis), starts it and follows it; exits with 1 when it did not end done
  phoenix-pnp job-diff FIRST SECOND [--out F] [--config F] [--lang CODE]   what differs between two files of job-export, part by part (the feeder, the nozzle, the batch, the tip, where it is picked and placed, the turn, what became of it, the order) and in the plan as a whole; exits with 1 when the jobs differ
  phoenix-pnp job-import FILE [--csv F] [--config F] [--lang CODE]   reads and checks such a file and says what it holds; --csv writes the placements with their status as a table; exits with 1 when the file is not valid
  phoenix-pnp parts-diff FIRST SECOND [--tol MM] [--rot-tol DEG] [--out F] [--config F] [--lang CODE]   what changed between two tables of parts, part by part: added, removed, moved, turned, another value, another package or the other side; exits with 1 when they differ
  phoenix-pnp config-diff FIRST SECOND [--lang CODE]   the settings that two configuration files give differently, in what they mean (yes and on, 5 and 5.0 and a line that gives the default are the same); exits with 1 when the machines differ
  phoenix-pnp board-stats PARTS [--near MM] [--limit N] [--config F] [--lang CODE]   what a table of parts holds (the sides, the kinds, each package, the box of the centres) and, with --near, the pairs of parts of one side whose centres are closer than MM; exits with 1 when it finds a pair
  phoenix-pnp queue-stock QUEUE [--out F] [--config F] [--lang CODE]   whether the feeders hold enough parts for all the jobs of the queue that are still to be made, one after the other, and the job in which each reel that falls short runs out; exits with 1 when one does
  phoenix-pnp feeder-deplete FEEDERS --state F [--out F] [--config F] [--lang CODE]   the table of feeders as it stands after the job of a saved state: what is left in each feeder and the next place of each strip; exits with 1 when a feeder is low or empty
  phoenix-pnp table-revision FILE [--config F]   the revision of a table (16 hexadecimal digits of its content, whatever its line ends): a write that carries it, as feeder-refill --out F --revision TOKEN, replaces the file only if the file still has that revision
  phoenix-pnp feeder-refill FEEDERS REFILLS [--out F] [--revision TOKEN] [--config F] [--lang CODE]   the table of feeders after reels were changed: REFILLS is a table id,count[,x,y] with a row for each feeder that got a new reel (the count of the new reel and, for a strip, where its first part is); everything else in the table is kept
  phoenix-pnp queue-add QUEUE NAME SOURCE FEEDERS [--panel] [--boards N] [--side top|bottom] [--config F] [--packages F] [--catalog F] [--relative]   puts a job (a table of parts, or a panel with --panel) at the end of a queue of jobs
  phoenix-pnp queue-status QUEUE [--check] [--config F]   the jobs of the queue and what became of each; --check plans the ones still to be made and exits with 1 when one cannot run now
  phoenix-pnp queue-confirm QUEUE NAME   records that the plan of the job, as it is now, was looked at and may be run (void when the files change)
  phoenix-pnp queue-edit QUEUE NAME skip|reset|revoke|remove   leaves a job out, puts it back to the start, takes its confirmation back or takes it out
  phoenix-pnp queue-run QUEUE [--only NAME] [--config F]   makes the jobs one board at a time on the simulated machine, writing the queue after every board; a job whose plan was not confirmed is not run (queue_confirm)
  phoenix-pnp nozzle-calibrate FILE [--config F] [--lang CODE]   the runout of the nozzle tip from readings at several angles (angle_deg,x_mm,y_mm)
  phoenix-pnp bom-order BOM [--boards N] [--spare-percent P] [--spare-min N] [--parts PARTS] [--have FEEDERS] [--out F] [--config F] [--lang CODE]   the parts to buy for N boards, with a spare (less what the feeders already hold)
  phoenix-pnp stock PARTS FEEDERS [--boards N] [--side top|bottom] [--out F] [--config F] [--lang CODE]   whether the feeders hold enough parts for N boards, kind by kind; exits with 1 when some fall short
  phoenix-pnp nozzle-life FILE [--warn N] [--reset TIP] [--config F] [--lang CODE]   the parts each nozzle tip has placed (a file kept beside the configuration); exits with 1 when a tip is at or over the warning level
  phoenix-pnp feeder-teach strip|tray [places and counts] | --measures FILE [--lang CODE]   the pitch and the angle of a strip or a tray from places measured on the machine
  phoenix-pnp panelize BOARD --cols N --rows N --pitch-x MM --pitch-y MM [--rotation DEG] [--fiducials MM] [--board-size W,H] [--x-out P2,P5] [--out F] [--lang CODE]   a panel file for a grid of copies of one board
  phoenix-pnp slot-bank TABLE [--tape MM] [--out F] [--config F] [--lang CODE]   the banks of feeders: the slots each feeder takes, which are free, depleted or jammed, and where a tape of MM millimetres fits; exits with 2 when a feeder is depleted or jammed
  phoenix-pnp panel-compose SPEC [--clearance MM] [--out F] [--config F] [--lang CODE]   checks a panel of boards put and turned on a sheet with rails (inside the rails, no overlap, a least distance) and writes the panel file; exits with 2 and writes nothing when it does not fit
  phoenix-pnp node-sim [--steps N] [--seed S] [--silence PM] [--unauthorized PM] [--interlock PM] [--config F]   a job on a head node and a feeder node that are made up, with faults drawn from the seed (per thousand requests); exits with 2 when the link was held
  phoenix-pnp part-inspect READINGS [--sizes F] [--packages F] [--out F] [--config F] [--lang CODE]   the parts on the nozzle (ref,package,long_mm,short_mm,dx_mm,dy_mm,rot_deg) against the sizes of their packages (a table of sizes, or the columns body_long_mm and body_short_mm of a table of packages)
  phoenix-pnp fiducial-check DESIGN MEASURED [--config F] [--lang CODE]   the turn, scale, shear and shift that the measured marks (id,x,y) put on the panel, against the limits
  phoenix-pnp vision-readings PARTS [--sizes F] [--packages F] [--vision F] [--seed N] [--spread MM] [--turn DEG] [--wrong-every N] [--out F] [--config F] [--lang CODE]   the readings that a simulated camera (with the noise, blur and failures of the vision settings) gives of the parts of a position file, in the file that part-inspect reads
  phoenix-pnp vision-marks DESIGN [--shift X,Y] [--turn DEG] [--scale S] [--dot MM] [--vision F] [--seed N] [--out F] [--config F] [--lang CODE]   the measured places of the marks of a panel that lies as said, as a simulated camera reads them, in the file that fiducial-check reads
  phoenix-pnp feeder-check FEEDERS [--packages F] [--catalog F] [--config F] [--lang CODE]   whether the table of feeders agrees with itself (slots, pitches, cells, places that clash or are out of reach)
  phoenix-pnp polarity-sheet PARTS [--side top|bottom] [--out F] [--config F] [--lang CODE]   the parts that have a direction, with their place and turn, as a CSV for the look after a job
  phoenix-pnp import-centroid FILE [--format auto|csv|eagle|kicad-pos] [--map F] [--dnp R1-R5,C7] [--out F] [--config F] [--lang CODE]   a position file of a CAD program as the table of parts
  phoenix-pnp vision-test [--vision F] [--size WxH] [--part LxS] [--mm-per-px MM] [--dx MM] [--dy MM] [--turn DEG] [--noise N] [--seed N] [--lang CODE]   measures the picture of a part made in memory and prints the error

OPTIONS of schedule, preflight, simulate, loadsheet and setup-sheet: --config FILE (the machine settings), --packages FILE and --catalog FILE (the tables of packages and parts), --panel FILE (a panel in place of
PARTS), --dnp R1,R2 (references to leave out as not to be mounted; a column dnp, fitted or populate in a CSV does the same), --fiducials FILE (with --panel: the measured places of the fiducials, a CSV id,x,y; an id is a mark of the panel, or board/reference for a part that is a mark of its board, such as A/FID1), --x-out A,B/C (with --panel: boards or panels of the panel to leave out as bad; a child with x_out set to true in the panel file is left out too), --side top|bottom, --board-width MM, --origin-x MM, --nozzles N, --speed MM_S, --order NAME and --lang CODE (the language of the error messages: en es de fr it ja zh pt). schedule and
simulate use the whole configuration (ranks, nozzle tips, the limits of the nozzle, the time of the moves) and print the checks before the job; they stop on an error unless --partial is given.
simulate runs the plan on a simulated machine; --journal FILE writes one row for every step (the part, the feeder, the nozzle, the second of the clock, how long it took and how it ended; a file that exists is never written over); --vision FILE gives the steps of the vision pipeline (the settings vision = pipeline and align = on make the simulated camera measure a picture of each part); --recipe FILE gives the recipe of the job (the steps it goes through and the parameters by package, feeder and tip; a recipe with the align step and without it turns that step on and off). With --state the job is saved when it stops (after N steps, or on an error) and resumed from that file the next time. --report-text writes the work report as text in the language of --lang, --report-html the same as a page that holds everything it shows (a file that exists is never written over), --report-csv and --report with the sections of the report after the statistics. --report-feeders writes the picks of each feeder as a CSV (planned and made, failures, the share of failures; a feeder over feeder_fail_percent is marked and named in the output).

import-centroid reads a centroid CSV (--map FILE names its columns, see examples/centroid.map), the text of positions of EAGLE or the .pos of KiCad (--format auto tells them apart) and writes the table of parts ref,val,package,x,y,rot,side,dnp in millimetres to --out (a file that exists is never written over) or to the standard output; --dnp marks references and ranges (R1-R5) as not to be mounted. The settings import_unit and import_bottom_mirror_width_mm of the configuration say what a file that gives no unit holds and whether the X of the bottom side is mirrored.

FEEDERS is a CSV with the header id,value,package,x,y,count. plan refuses to write when a part has no feeder or a feeder runs out, unless --partial is given
(then only the parts that can be placed are in the plan). The bottom side needs --board-width to be mirrored, about the middle of a board that starts at X 0 (--origin-x MM says where its left edge is).

Examples (each one is run by a test, from the folder of the repository):
  example: phoenix-pnp parts examples/board.pos
  example: phoenix-pnp check examples/board.pos examples/feeders.csv
  example: phoenix-pnp board-stats examples/board.pos
  example: phoenix-pnp slot-bank examples/slot_bank.csv --tape 8
  example: phoenix-pnp panel-compose examples/panel_compose.json
  example: phoenix-pnp node-sim --seed 7 --steps 10
Exit codes: 0 when all is well, 1 when an argument or a file is wrong or the result differs, 2 when a check of the command is not passed.";

fn read(path: &str) -> Result<String, String> {
    commands::read_text(path, MachineConfig::default().max_file_bytes)
}

/// Opens the window with the files named. Without the window in this build, no command shows the help and `gui` says why it cannot open.
#[cfg(feature = "gui")]
fn open_window(files: &[String]) -> Result<(), String> {
    let paths: Vec<std::path::PathBuf> = files.iter().map(std::path::PathBuf::from).collect();
    eh3d_phoenix_pnp::gui::window::run(&paths)
        .map_err(|e| format!("the window could not open: {e}"))
}

#[cfg(not(feature = "gui"))]
fn open_window(files: &[String]) -> Result<(), String> {
    if files.is_empty() {
        println!("{HELP}");
        return Ok(());
    }
    Err("this build has no window; build it with --features gui".into())
}

fn run(mut args: Vec<String>) -> Result<(), String> {
    if args.is_empty() {
        return open_window(&[]);
    }
    if matches!(args[0].as_str(), "help" | "--help" | "-h") {
        println!("{HELP}");
        return Ok(());
    }
    let cmd = args.remove(0);
    match cmd.as_str() {
        "gui" => {
            if cfg!(feature = "gui") {
                open_window(&args)
            } else {
                Err("this build has no window; build it with --features gui".into())
            }
        }
        "parts" => {
            finish(&args, 1)?;
            for p in parse(&read(args.first().ok_or("parts needs a file")?)?)? {
                println!(
                    "{:<8} {:<14} {:<16} {:>9.3} {:>9.3} {:>6.1}  {:?}",
                    p.reference, p.value, p.package, p.x, p.y, p.rotation, p.side
                );
            }
            Ok(())
        }
        "check" | "plan" => {
            let mut s = Settings::default();
            if let Some(v) = flag(&mut args, "--nozzles")? {
                let n = num(&v, "--nozzles")?;
                if n.fract() != 0.0 || !(1.0..=6.0).contains(&n) {
                    return Err(format!("--nozzles {v} is not a whole number from 1 to 6"));
                }
                s.nozzles = n as usize;
            }
            if let Some(v) = flag(&mut args, "--speed")? {
                s.speed_mm_s = num(&v, "--speed")?;
            }
            if let Some(v) = flag(&mut args, "--board-width")? {
                s.board_width_mm = Some(num(&v, "--board-width")?);
            }
            if let Some(v) = flag(&mut args, "--side")? {
                s.side = match v.as_str() {
                    "top" => Side::Top,
                    "bottom" => Side::Bottom,
                    other => return Err(format!("--side '{other}' is not top or bottom")),
                };
            }
            if let Some(v) = flag(&mut args, "--origin-x")? {
                s.origin_x_mm = num(&v, "--origin-x")?;
            }
            let partial = switch(&mut args, "--partial");
            if args.len() < 2 {
                return Err(format!("{cmd} needs the parts and the feeders"));
            }
            finish(&args, if cmd == "check" { 2 } else { 3 })?;
            let parts = parse(&read(&args[0])?)?;
            let feeders = feeders_csv(&read(&args[1])?)?;
            // a plan is for one side: the other side's parts neither need feeders nor use them up
            let a = if cmd == "plan" {
                assign_side(&parts, &feeders, Some(s.side))
            } else {
                assign(&parts, &feeders)
            };
            for r in &a.without_feeder {
                println!("no feeder for {r}");
            }
            for (id, n) in &a.short {
                println!("feeder {id} is short of {n} parts");
            }
            let clean = a.without_feeder.is_empty() && a.short.is_empty();
            if cmd == "check" {
                return if clean {
                    println!("every part has a feeder with enough parts");
                    Ok(())
                } else {
                    Err("the feeders do not cover the board".into())
                };
            }
            if !clean && !partial {
                return Err("the feeders do not cover the board (use --partial to place only what can be placed)".into());
            }
            let out = args.get(2).ok_or("plan needs an output file")?;
            // the plan never goes over a file it was made from
            let same_file = |a: &str, b: &str| match (
                std::path::Path::new(a).canonicalize(),
                std::path::Path::new(b).canonicalize(),
            ) {
                (Ok(x), Ok(y)) => x == y,
                _ => a == b,
            };
            if args[..2].iter().any(|input| same_file(input, out)) {
                return Err(format!(
                    "{out}: the output file is also an input; write the plan to a new file"
                ));
            }
            let p = plan(&parts, &feeders, &a, &s)?;
            // written beside and renamed, so that a failure leaves the old plan whole
            let tmp = format!("{out}.tmp");
            fs::write(&tmp, p.to_json())
                .and_then(|()| fs::rename(&tmp, out))
                .map_err(|e| {
                    let _ = fs::remove_file(&tmp);
                    format!("{out}: {e}")
                })?;
            println!(
                "{} parts in {} batches, {:.0} mm of travel, about {:.0} s",
                p.placements.len(),
                p.batches,
                p.travel_mm,
                p.seconds
            );
            Ok(())
        }
        "schedule" => commands::schedule(args),
        "preflight" => commands::run_preflight(args),
        "simulate" => commands::simulate(args),
        "loadsheet" => commands::run_loadsheet(args),
        "setup-sheet" => commands::setup_sheet(args),
        "order-compare" => commands::order_compare(args),
        "config-check" => commands::config_check(args),
        "check-bottom" => commands::check_bottom(args),
        "tables-check" => commands::tables_check(args),
        "recipe-check" => commands::recipe_check(args),
        "import-centroid" => commands::import_centroid(args),
        "camera-calibrate" => cmd_tools::camera_calibrate(args),
        "camera-pattern" => cmd_tools::camera_pattern(args),
        "camera-combined" => cmd_tools::camera_combined(args),
        "parts-diff" => cmd_more::parts_diff(args),
        "config-diff" => cmd_more::config_diff(args),
        "board-stats" => cmd_more::board_stats(args),
        "queue-stock" => cmd_more::queue_stock(args),
        "feeder-deplete" => cmd_more::feeder_deplete(args),
        "feeder-refill" => cmd_more::feeder_refill(args),
        "package-gen" => cmd_tools::package_gen(args),
        "package-kicad" => cmd_tools::package_kicad(args),
        "place-learn" => cmd_tools::place_learn(args),
        "job-export" => commands::job_export(args),
        "aside-sheet" => commands::aside_sheet(args),
        "feeder-slots" => commands::feeder_slots(args),
        "simulate-runs" => commands::simulate_runs(args),
        "reel-runway" => commands::reel_runway(args),
        "nozzle-compare" => commands::nozzle_compare(args),
        "simulate-hotspots" => commands::simulate_hotspots(args),
        "queue-changeover" => cmd_more::queue_changeover(args),
        "job-diff" => cmd_more::job_diff(args),
        "camera-selftest" => cmd_flow::camera_selftest(args),
        "machine-template" => cmd_flow::machine_template(args),
        "lot-trace" => cmd_flow::lot_trace(args),
        "export-assembler" => cmd_flow::export_assembler(args),
        "split-job" => cmd_flow::split_job(args),
        "failure-odds" => cmd_flow::failure_odds(args),
        "pos-transform" => cmd_flow::pos_transform(args),
        "server-request" => cmd_server::server_request(args),
        "server-send" => cmd_server::server_send(args),
        "server-status" => cmd_server::server_status(args),
        "server-job" => cmd_server::server_job(args),
        "server-feeders" => cmd_server::server_feeders(args),
        "nozzle-zcal" => cmd_flow::nozzle_zcal(args),
        "panel-fids" => cmd_flow::panel_fids(args),
        "import-assembler" => cmd_flow::import_assembler(args),
        "bom-merge" => cmd_flow::bom_merge(args),
        "job-fingerprint" => cmd_shop::job_fingerprint(args),
        "serial-sign" => cmd_shop::serial_sign(args),
        "serial-verify" => cmd_shop::serial_verify(args),
        "shift-plan" => commands::shift_plan(args),
        "head-split" => cmd_shop::head_split(args),
        "footprint-check" => cmd_shop::footprint_check(args),
        "reject-report" => cmd_shop::reject_report(args),
        "retry-policy" => cmd_shop::retry_policy(args),
        "clearance" => cmd_shop::clearance_check(args),
        "bom-cost" => cmd_shop::bom_cost(args),
        "lot-age" => cmd_shop::lot_age(args),
        "export-fab" => cmd_fab::export_fab(args),
        "fab-check" => cmd_fab::fab_check(args),
        "convert-cpl" => cmd_fab::convert_cpl(args),
        "ipc-name" => cmd_fab::ipc_name(args),
        "footprint-find" => cmd_fab::footprint_find(args),
        "dfm-check" => cmd_fab::dfm_check(args),
        "stencil-apertures" => cmd_fab::stencil_apertures(args),
        "sim-busy" => cmd_fab::sim_busy(args),
        "sim-tower" => cmd_fab::sim_tower(args),
        "driver-check" => cmd_drive::driver_check(args),
        "driver-test" => cmd_drive::driver_test(args),
        "driver-run" => cmd_drive::driver_run(args),
        "safety-replay" => cmd_drive::safety_replay(args),
        "homing-repeat" => cmd_drive::homing_repeat(args),
        "bringup" => cmd_drive::bringup(args),
        "import-openpnp" => cmd_import::import_openpnp(args),
        "assist" => cmd_import::assist_cmd(args),
        "align-iterate" => cmd_import::align_iterate(args),
        "vacuum-calibrate" => cmd_import::vacuum_calibrate(args),
        "test-board-gen" => cmd_import::test_board_gen(args),
        "server-discover" => cmd_server::server_discover(args),
        "tape-steps" => commands::tape_steps(args),
        "shift-oee" => commands::shift_oee(args),
        "server-calibrate" => cmd_server::server_calibrate(args),
        "job-import" => cmd_tools::job_import(args),
        "queue-add" => cmd_queue::queue_add(args),
        "queue-status" => cmd_queue::queue_status(args),
        "queue-confirm" => cmd_queue::queue_confirm(args),
        "queue-edit" => cmd_queue::queue_edit(args),
        "queue-run" => cmd_queue::queue_run(args),
        "nozzle-calibrate" => cmd_tools::nozzle_calibrate(args),
        "panelize" => cmd_tools::panelize(args),
        "polarity-sheet" => cmd_tools::polarity_sheet(args),
        "bom-order" => cmd_tools::bom_order(args),
        "feeder-teach" => cmd_tools::feeder_teach(args),
        "stock" => cmd_tools::stock(args),
        "nozzle-life" => cmd_tools::nozzle_life(args),
        "bom-check" => cmd_tools::bom_check(args),
        "part-inspect" => cmd_vis::part_inspect(args),
        "fiducial-check" => cmd_vis::fiducial_check(args),
        "vision-readings" => cmd_vis::vision_readings(args),
        "vision-marks" => cmd_vis::vision_marks(args),
        "feeder-check" => cmd_vis::feeder_check(args),
        "vision-test" => commands::vision_test(args),
        "table-revision" => cmd_nodes::table_revision(args),
        "slot-bank" => cmd_nodes::slot_bank(args),
        "panel-compose" => cmd_nodes::panel_compose(args),
        "node-sim" => cmd_nodes::node_sim(args),
        other => Err(format!("{other}: unknown command (see help)")),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if eh3d_phoenix_pnp::gui::shot::wants_picture(&args) {
        #[cfg(feature = "gui")]
        return ExitCode::from(eh3d_phoenix_pnp::gui::window::take_picture(&args));
        #[cfg(not(feature = "gui"))]
        {
            eprintln!("error: this build has no window; build it with --features gui");
            return ExitCode::from(1);
        }
    }
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(cmd_nodes::exit_code())
        }
    }
}

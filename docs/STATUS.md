# EH3D-PHOENIX-PNP: status

## Current state

EH3D-PHOENIX-PNP is a working program whose every machine-facing capability is a simulation: a library, a command line (`phoenix-pnp`) and a window (`phoenix-pnp-gui`, built with `--features gui`). It reads position files and tables, assigns feeders, plans the order of the placements, carries a job out on a simulated machine with a simulated camera, vacuum and G-code controller, and writes the plan, the load sheet, the report and the files for assembly houses. The suite of tests runs on Windows and Linux in the CI. Nothing has moved a real machine.

## Projects of the suite

The window has a panel «Attachments» (the shared `eh3d-attachpanel`): the data sheets, manuals and other files of the project are listed, attached, linked to a part, extracted and removed after a question, and shown beside the list (`eh3d-project.md`). The panel itself is tested in EH3D-COMMON; it has not been tried on a screen by a person.

The command `project` keeps the work in the branch `pnp/` of an `.eh3d` project (`eh3d-project.md`): the other branches keep their bytes, copies taken from them are told as out of date and imported files keep their original with a report.

## The window

The window is built on the docking frame of the suite (`eh3d-dock`, in EH3D-COMMON). Its panels are the seven of the sector of pick and place (the board view, parts, feeders, job, camera, machine and console) and the guide, packages, panels, boards, load sheet, tools and problems, and one panel for each of fifteen commands of the driver and the assistants, hidden until they are asked for. It starts in one of three arrangements, Setup, Run and Compact; the person can move, float, fold, hide, maximise and lock the panels and save an arrangement under a name, and what was arranged is kept in `window.cfg` under keys that begin with `dock.`. The menus and the shortcuts, the panels and the arrangements are data and are tested without a screen. The drawing has been compiled and run in a context with no window; no person has looked at the arrangements on a screen, and the colours do not yet have the look that the other programs of the suite share.

## Banks, nodes and panels

The slots of the left and right banks of feeders are modelled (how many slots a tape takes, that none overlaps, the reach of the head, and the states free, occupied, depleted and jammed). The head node and the feeder node of the suite are asked over HTTP through the common client, with a hold on a node that cannot be heard and an interlock of the axes that reads whether the head is clear; both are tried only against nodes on a computer. The window has a panel for the slots and one for the nodes, hidden at the start. A panel of boards is composed and checked from a file (`panel-compose`). The window reads a table of the banks from a file, lets the state of a feeder be changed and saves the table back to the file it came from only while the file is as it was read; every command that checks something ends with the code 2 when the check is not passed and with 1 when it could not be run. The hooks are told while a job goes, for every pick and place, and the journal, the Job tab and the signal tower of the simulation say what each nozzle is doing.

## Machine and firmware

A driver profile says whether the machine homes with end switches or without them (a sensitivity for X and one for Y, sent before the lines of `home`); `homing-repeat` shows how far apart a simulated machine lands after N homings. `fw-config-diff` compares two dumps of the configuration of a firmware, as Marlin answers it or as a Klipper `printer.cfg` is written (the values saved by `SAVE_CONFIG` win over the file above them), and warns to reset the EEPROM when the firmware changed. The state of a node is asked with the token and the time it waits before its safe state is read and told; a second link asks it while an order is waited for (`node-sim`, the panel of the nodes). `camera-shots` places the pictures of a part bigger than the camera and `or_opt` shortens a walk further. `toolpath-hash` gives the SHA-256 of the lines that a driver profile writes for a whole job and, with `--expect`, ends with 2 when they are not the lines that were seen; it is a local check only (`lines_sha256`). A job of the server is authorised with the hash that the server made of it (`server-status`, `server-job --authorize`), tried only against the server in memory and a server of the tests; the window does not authorise yet. A text of an exported CSV that starts like a formula carries a quote in front, taken off when the program reads it. The specification sheet of the machine (`machine-sheet`) comes from five lines of the settings; what nobody measured says so. Nothing of this has been tried on a machine: the controller is simulated and the dumps are examples.

## Capabilities

The official state of each capability is in `REQUIREMENTS.md`. The register has 177 requirements: 122 implemented, 34 prototype, 21 planned and 0 validated. By progress: 122 done, 34 partial, 15 pending and 6 blocked. This page never states a state that `REQUIREMENTS.md` does not.

## Open questions

* Whether a live camera will be a layer of this program or only of the vision program; today the camera is a model.
* Which controller families to describe after Marlin and Klipper; a profile for another dialect needs its answers captured from a real controller, which has not been done.
* How a sensor of a real machine (a door, the nozzles lifted) will be reported to the interlocks: today a program reports it through the session.
* Whether a process (the phases of a board) is kept on the server so that several programs see its state.

## What it needs from other repositories

* `EH3D-COMMON`: the client of the server, the panel of the settings of the server, the docking frame and the project container, as a sibling folder. The workflows take its tag v0.3.6.
* The vision program of the suite for a live camera; the nodes of the feeders and of the head, whose contracts this program follows, for anything that is not simulated: no real node has been asked.

## Known technical debt and omitted tests

* The machine parts are separate pieces but not formal traits (see the first requirement of the register).
* The reason (`why`) of an order that a node refuses with 409 and the token that stays after a `401` came with release 0.3.0 of the common client (`COMMON_REQUESTS.md`); they were tried only against nodes on a computer.
* The answers of the controllers in the examples are written by hand, not captured; the import of the files of OpenPnP and the readers of the position files have not been tried with real files.
* The drawing of the window is not covered by tests; the pictures of the README are taken by the program itself.
* Tests that need a machine, a camera or a screen are not written: the register names each as `planned:` with the test that will show it.

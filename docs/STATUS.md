# EH3D-PHOENIX-PNP: status

## Current state

EH3D-PHOENIX-PNP is a working program whose every machine-facing capability is a simulation: a library, a command line (`phoenix-pnp`) and a window (`phoenix-pnp-gui`, built with `--features gui`). It reads position files and tables, assigns feeders, plans the order of the placements, carries a job out on a simulated machine with a simulated camera, vacuum and G-code controller, and writes the plan, the load sheet, the report and the files for assembly houses. The suite of tests runs on Windows and Linux in the CI. Nothing has moved a real machine.

## The window

The window is built on the docking frame of the suite (`eh3d-dock`, in EH3D-COMMON). Its panels are the seven of the sector of pick and place (the board view, parts, feeders, job, camera, machine and console) and the guide, packages, panels, boards, load sheet, tools and problems, and one panel for each of eleven commands of the driver and the assistants, hidden until they are asked for. It starts in one of three arrangements, Setup, Run and Compact; the person can move, float, fold, hide, maximise and lock the panels and save an arrangement under a name, and what was arranged is kept in `window.cfg` under keys that begin with `dock.`. The menus and the shortcuts, the panels and the arrangements are data and are tested without a screen. The drawing has been compiled and run in a context with no window; no person has looked at the arrangements on a screen, and the colours do not yet have the look that the other programs of the suite share.

## Banks, nodes and panels

The slots of the left and right banks of feeders are modelled (how many slots a tape takes, that none overlaps, the reach of the head, and the states free, occupied, depleted and jammed). The head node and the feeder node of the suite are asked over HTTP through the common client, with a hold on a node that cannot be heard and an interlock of the axes that reads whether the head is clear; both are tried only against nodes on a computer. The window has a panel for the slots and one for the nodes, hidden at the start. A panel of boards is composed and checked from a file (`panel-compose`). The window reads a table of the banks from a file, lets the state of a feeder be changed and saves the table back to the file it came from only while the file is as it was read; every command that checks something ends with the code 2 when the check is not passed and with 1 when it could not be run. The hooks are told while a job goes, for every pick and place, and the journal says what each nozzle is doing.

## Capabilities

The official state of each capability is in `REQUIREMENTS.md`. The register has 148 requirements: 93 implemented, 33 prototype, 22 planned and 0 validated. By progress: 93 done, 33 partial, 16 pending and 6 blocked. This page never states a state that `REQUIREMENTS.md` does not.

## Open questions

* Whether a live camera will be a layer of this program or only of the vision program; today the camera is a model.
* Which controller families to describe after Marlin and Klipper; a profile for another dialect needs its answers captured from a real controller, which has not been done.
* How a sensor of a real machine (a door, the nozzles lifted) will be reported to the interlocks: today a program reports it through the session.
* Whether a process (the phases of a board) is kept on the server so that several programs see its state.

## What it needs from other repositories

* `EH3D-COMMON`: the client of the server, the panel of the settings of the server and the docking frame, as a sibling folder at version 0.2.7.
* The vision program of the suite for a live camera; the nodes of the feeders and of the head, whose contracts this program follows, for anything that is not simulated: no real node has been asked.

## Known technical debt and omitted tests

* The machine parts are separate pieces but not formal traits (see the first requirement of the register).
* The common client keeps only the `error` and `detail` of a refusal and forgets a token after a `401`, so the reason (`why`) of an order that a node refuses with 409 is not seen through it; the change asked of the client is in `COMMON_REQUESTS.md`.
* The answers of the controllers in the examples are written by hand, not captured; the import of the files of OpenPnP and the readers of the position files have not been tried with real files.
* The drawing of the window is not covered by tests; the pictures of the README are taken by the program itself.
* Tests that need a machine, a camera or a screen are not written: the register names each as `planned:` with the test that will show it.

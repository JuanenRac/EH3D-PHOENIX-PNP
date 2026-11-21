# EH3D-PHOENIX-PNP: not yet

Good ideas that are not in the current version. Writing them here keeps anybody from implementing them early or twice.

| Idea | Why not yet | Revisit when |
|---|---|---|
| A real serial or network connection to a controller | The driver has only been tried against a simulated controller; a connection needs a machine and the rules of safety shown on it | a machine is there for a first start, with the steps of `bringup` |
| Writing a board file for OpenPnP | The import is read only; writing a file for another program needs the format checked against that program | a person asks to compare results between the two programs |
| A live camera | The vision program of the suite owns the capture, with its own backends for each system | the vision program has a live layer that this one can read |
| A link to real nodes of the feeders and of the head | The link and the nodes on a computer exist; the reason of a refusal and the fixed token come from the common client (`COMMON_REQUESTS.md`), but no real node has answered | a node is wired and a person can try the steps of `bringup` against it |
| Paste dispensing | It comes after the camera and the driver are tried on a machine | both work on a machine |
| A process kept on the server, with the load-out of each phase saved there | The phases and their rules exist and are kept in a file; the server does not store a phase | the task of the server for pick and place keeps a phase |
| Variants of a board built from design data, bills of materials and rules in one command | It is large; the pieces (merging bills, converting position files, searching footprints) exist | the design tool exports the variants of a board |
| A panel with the tooling holes and the offsets of the board files | The composition has the sheet, the rails and the units; the holes and the offset of each board file come with the panel of the design tool | the design tool exports a panel |
| The events of the camera for the hooks (before and after a capture, before it settles) | The camera is a model; there is no capture to tell | the vision program has a live layer that this one can read |
| The cost of a part as a key of the order of the places | The prices are in their own table, which the planner does not read | the planner reads the table of prices |
| A made-up board with QFP chips and a panel of two designs with two variants | `test-board-gen` makes a grid, a mix and a dense board; a panel with variants needs a panel file that has designs with variants | the composition of a panel reads designs with variants |
| Starting points per family of machine that also give the steps per millimetre and the homing | The templates give the area, the speeds and the accelerations that the plan uses; the steps per millimetre belong to the firmware and the homing to the driver profile, which no template writes | a template can write a driver profile |
| Comparing a Klipper file that a real controller saved | `fw-config-diff` reads the files of Klipper as they are written by hand and the block that `SAVE_CONFIG` adds; no file saved by a real controller has been read | a person has two files from a real Klipper machine to compare |
| The homing without switches tried on a motor | The profile, the lines and the spread of the landing places are tried on the simulated controller; no driver of a motor has felt a stop | a machine is there to repeat the homing with `homing-repeat` |
| The revision on every table that is written | The table of feeders, of the banks, of parts made by an import and the queue take a revision; the other commands write new files and never over one | a command replaces a table that it did not make |
| A feeder with a drag pitch found by the camera | It needs the live camera and the model of a feeder with a pitch | after the live camera |
| Fixing the origin of a board holder by touching its holes with the camera | It needs the live camera | after the live camera |
| A real tower of lights | The logic of the tower exists; no node drives a lamp | a node can drive it |
| Interlocks read from the sensors of a real machine | The rule and the session exist; no sensor of a real door has been read | a machine has the sensors |
| Jogging the head with the keys of the window | It needs a machine to jog and the safety rules of a move by hand | a machine is connected |
| The height map applied to the height of a placement in a job | `height-map` gives the change of height of each placement from a grid that a person typed; the planner and the driver do not read it | a sensor on the head can measure the grid on a machine |
| The pick places of a drag feeder read by the planner | `drag-plan` works out the stroke and the places of one stroke; the plan still picks a strip at the place a `strip` gives | a drag feeder is tried on a machine with the camera of the vision program |
| Matching the fingerprint of the lines with the one the server holds | `toolpath-hash` fingerprints the lines of this program; the server fingerprints its own request, and the two are different numbers | the server accepts the fingerprint of the lines of a program as its own |
| Equivalent parts found by the program | `equivalents` takes a table of groups that a person writes; nothing says which parts may stand for each other | the bill of materials carries the equivalents that the design allows |

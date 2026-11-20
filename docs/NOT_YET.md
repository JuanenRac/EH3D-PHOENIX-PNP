# EH3D-PHOENIX-PNP: not yet

Good ideas that are not in the current version. Writing them here keeps anybody from implementing them early or twice.

| Idea | Why not yet | Revisit when |
|---|---|---|
| A real serial or network connection to a controller | The driver has only been tried against a simulated controller; a connection needs a machine and the rules of safety shown on it | a machine is there for a first start, with the steps of `bringup` |
| Writing a board file for OpenPnP | The import is read only; writing a file for another program needs the format checked against that program | a person asks to compare results between the two programs |
| A live camera | The vision program of the suite owns the capture, with its own backends for each system | the vision program has a live layer that this one can read |
| Nodes for the feeders and for the head | The nodes do not exist yet; the program describes them with data and a simulated bus | the nodes are created |
| Paste dispensing | It comes after the camera and the driver are tried on a machine | both work on a machine |
| A process kept on the server, with the load-out of each phase saved there | The phases and their rules exist and are kept in a file; the server does not store a phase | the task of the server for pick and place keeps a phase |
| Variants of a board built from design data, bills of materials and rules in one command | It is large; the pieces (merging bills, converting position files, searching footprints) exist | the design tool exports the variants of a board |
| A composed panel with rails and a different design in each unit | The grid and the panel file with boards as children cover most uses | a panel with two designs has to be assembled |
| A feeder with a drag pitch found by the camera | It needs the live camera and the model of a feeder with a pitch | after the live camera |
| Fixing the origin of a board holder by touching its holes with the camera | It needs the live camera | after the live camera |
| A real tower of lights | The logic of the tower exists; no node drives a lamp | a node can drive it |
| Interlocks read from the sensors of a real machine | The rule and the session exist; no sensor of a real door has been read | a machine has the sensors |
| Jogging the head with the keys of the window | It needs a machine to jog and the safety rules of a move by hand | a machine is connected |

------------------------------ MODULE Todo ------------------------------
(* A to-do list, as a state machine.                                     *)
(*                                                                       *)
(* Every task is in exactly one of three states, and this says which     *)
(* moves between them are allowed. It says nothing about storage, HTTP,  *)
(* React, or what the buttons are called — only what may happen.         *)
EXTENDS FiniteSets

CONSTANT Ids

Absent == "absent"
Open   == "open"
Done   == "done"

VARIABLE tasks

TypeOK == tasks \in [Ids -> {Absent, Open, Done}]

Init == tasks = [i \in Ids |-> Absent]

(* A task can only be added if it is not already there. *)
Add(i) ==
  /\ tasks[i] = Absent
  /\ tasks' = [tasks EXCEPT ![i] = Open]

(* Only an open task can be completed. Completing a task that was never
   added, or is already done, is not a thing that happens. *)
Complete(i) ==
  /\ tasks[i] = Open
  /\ tasks' = [tasks EXCEPT ![i] = Done]

Reopen(i) ==
  /\ tasks[i] = Done
  /\ tasks' = [tasks EXCEPT ![i] = Open]

Delete(i) ==
  /\ tasks[i] # Absent
  /\ tasks' = [tasks EXCEPT ![i] = Absent]

(* "Clear completed" removes the done tasks and leaves everything else
   alone. That second half is the part implementations get wrong. *)
ClearCompleted ==
  /\ \E i \in Ids : tasks[i] = Done
  /\ tasks' = [i \in Ids |-> IF tasks[i] = Done THEN Absent ELSE tasks[i]]

Next ==
  \/ \E i \in Ids : Add(i) \/ Complete(i) \/ Reopen(i) \/ Delete(i)
  \/ ClearCompleted

Spec == Init /\ [][Next]_tasks
=========================================================================

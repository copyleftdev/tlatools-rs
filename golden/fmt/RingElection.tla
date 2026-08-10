---- MODULE RingElection ----
EXTENDS Naturals, FiniteSets
CONSTANTS Node, Ident, Succ
IdSet == {Ident[n] : n \in Node}
MaxId == CHOOSE i \in IdSet : \A j \in IdSet : j <= i
VARIABLES inbox, leader
TypeOK == inbox \in [Node -> SUBSET IdSet] /\ leader \subseteq Node
Init == inbox = [n \in Node |-> {Ident[m] : m \in {x \in Node : Succ[x] = n}}] /\ leader = {}
Forward(n, v) == v \in inbox[n] /\ v > Ident[n] /\ inbox' = [inbox EXCEPT ![n] = @ \ {v}, ![Succ[n]] = @ \cup {v}] /\ (UNCHANGED leader)
Discard(n, v) == v \in inbox[n] /\ v < Ident[n] /\ inbox' = [inbox EXCEPT ![n] = @ \ {v}] /\ (UNCHANGED leader)
Elect(n, v) == v \in inbox[n] /\ v = Ident[n] /\ inbox' = [inbox EXCEPT ![n] = @ \ {v}] /\ leader' = leader \cup {n}
Next == \E n \in Node, v \in IdSet : Forward(n, v) \/ Discard(n, v) \/ Elect(n, v)
Spec == Init /\ ([][Next]_<<inbox, leader>>)
OnlyMaxCanWin == \A n \in leader : Ident[n] = MaxId
AtMostOneLeader == Cardinality(leader) <= 1
====

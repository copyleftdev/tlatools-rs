----------------------------- MODULE Paxos -----------------------------
(* Single-decree Paxos. The subtle rule is Phase2a: a proposer may only     *)
(* propose a value that is safe at its ballot -- the value accepted at the  *)
(* highest ballot reported by its quorum, or any value if none reported.    *)
EXTENDS Integers, FiniteSets

CONSTANTS Acceptor, Value, Ballot, Quorum

NoBallot == -1
NoValue  == "none"

Message ==
       [type : {"1a"}, bal : Ballot]
  \cup [type : {"1b"}, acc : Acceptor, bal : Ballot,
        mbal : Ballot \cup {NoBallot}, mval : Value \cup {NoValue}]
  \cup [type : {"2a"}, bal : Ballot, val : Value]
  \cup [type : {"2b"}, acc : Acceptor, bal : Ballot, val : Value]

VARIABLES maxBal, maxVBal, maxVal, msgs

TypeOK ==
  /\ maxBal  \in [Acceptor -> Ballot \cup {NoBallot}]
  /\ maxVBal \in [Acceptor -> Ballot \cup {NoBallot}]
  /\ maxVal  \in [Acceptor -> Value \cup {NoValue}]
  /\ msgs \subseteq Message

Init ==
  /\ maxBal  = [a \in Acceptor |-> NoBallot]
  /\ maxVBal = [a \in Acceptor |-> NoBallot]
  /\ maxVal  = [a \in Acceptor |-> NoValue]
  /\ msgs = {}

Send(m) == msgs' = msgs \cup {m}

Phase1a(b) ==
  /\ Send([type |-> "1a", bal |-> b])
  /\ UNCHANGED <<maxBal, maxVBal, maxVal>>

Phase1b(a) ==
  \E m \in msgs :
    /\ m.type = "1a"
    /\ m.bal > maxBal[a]
    /\ maxBal' = [maxBal EXCEPT ![a] = m.bal]
    /\ Send([type |-> "1b", acc |-> a, bal |-> m.bal,
             mbal |-> maxVBal[a], mval |-> maxVal[a]])
    /\ UNCHANGED <<maxVBal, maxVal>>

Phase2a(b, v) ==
  /\ ~ \E m \in msgs : m.type = "2a" /\ m.bal = b
  /\ \E Q \in Quorum :
       LET Q1b == {m \in msgs : m.type = "1b" /\ m.acc \in Q /\ m.bal = b}
       IN  /\ \A a \in Q : \E m \in Q1b : m.acc = a
           /\ \/ \A m \in Q1b : m.mbal = NoBallot
              \/ \E m \in Q1b :
                   /\ m.mval = v
                   /\ \A mm \in Q1b : m.mbal >= mm.mbal
  /\ Send([type |-> "2a", bal |-> b, val |-> v])
  /\ UNCHANGED <<maxBal, maxVBal, maxVal>>

Phase2b(a) ==
  \E m \in msgs :
    /\ m.type = "2a"
    /\ m.bal >= maxBal[a]
    /\ maxBal'  = [maxBal  EXCEPT ![a] = m.bal]
    /\ maxVBal' = [maxVBal EXCEPT ![a] = m.bal]
    /\ maxVal'  = [maxVal  EXCEPT ![a] = m.val]
    /\ Send([type |-> "2b", acc |-> a, bal |-> m.bal, val |-> m.val])

Next ==
  \/ \E b \in Ballot : Phase1a(b) \/ \E v \in Value : Phase2a(b, v)
  \/ \E a \in Acceptor : Phase1b(a) \/ Phase2b(a)

Spec == Init /\ [][Next]_<<maxBal, maxVBal, maxVal, msgs>>

VotedFor(a, b, v) == [type |-> "2b", acc |-> a, bal |-> b, val |-> v] \in msgs
ChosenAt(b, v)    == \E Q \in Quorum : \A a \in Q : VotedFor(a, b, v)
Chosen            == {v \in Value : \E b \in Ballot : ChosenAt(b, v)}

(* The whole point of the protocol. *)
Consistency == Cardinality(Chosen) <= 1

OneProposalPerBallot ==
  \A m1, m2 \in msgs :
    (m1.type = "2a" /\ m2.type = "2a" /\ m1.bal = m2.bal) => m1.val = m2.val
========================================================================

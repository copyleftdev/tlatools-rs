----------------------------- MODULE TV -----------------------------
EXTENDS Integers, Sequences, FiniteSets, TLC

VARIABLES rmState, tmState, tmPrepared, msgs, tv_i, tv_p

S == INSTANCE TwoPhase WITH RM <- {"r1", "r2"}

Traces ==
  <<
    << [msgs |-> {}, rmState |-> [r1 |-> "working", r2 |-> "working"], tmPrepared |-> {}, tmState |-> "init"],
       [msgs |-> {[rm |-> "r1", type |-> "Prepared"]}, rmState |-> [r1 |-> "prepared", r2 |-> "working"], tmPrepared |-> {}, tmState |-> "init"] >>,
    << [msgs |-> {}, rmState |-> [r1 |-> "working", r2 |-> "working"], tmPrepared |-> {}, tmState |-> "init"],
       [msgs |-> {[type |-> "Abort"]}, rmState |-> [r1 |-> "working", r2 |-> "working"], tmPrepared |-> {}, tmState |-> "done"] >>
  >>

StateEq(s)     == rmState = s.rmState /\ tmState = s.tmState /\ tmPrepared = s.tmPrepared /\ msgs = s.msgs
StateEqPrim(s) == rmState' = s.rmState /\ tmState' = s.tmState /\ tmPrepared' = s.tmPrepared /\ msgs' = s.msgs

CurPath == Traces[tv_p]
tvars   == <<rmState, tmState, tmPrepared, msgs, tv_i, tv_p>>

TraceInit == /\ tv_p \in 1..Len(Traces)
             /\ tv_i = 1
             /\ StateEq(Traces[tv_p][1])

TraceNext == /\ tv_i < Len(CurPath)
             /\ tv_i' = tv_i + 1
             /\ S!TPNext
             /\ StateEqPrim(CurPath[tv_i + 1])
             /\ UNCHANGED tv_p

TraceSpec == TraceInit /\ [][TraceNext]_tvars /\ WF_tvars(TraceNext)
Complete  == tv_i = Len(CurPath)
Refines   == <>Complete
========================================================================

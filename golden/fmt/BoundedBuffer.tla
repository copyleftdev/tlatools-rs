---- MODULE BoundedBuffer ----
EXTENDS Naturals, Sequences
CONSTANTS Capacity, MaxItems
VARIABLES buf, nextVal, received
TypeOK == buf \in Seq(1..MaxItems) /\ nextVal \in 1..MaxItems + 1 /\ received \in Seq(1..MaxItems)
Init == buf = <<>> /\ nextVal = 1 /\ received = <<>>
Put == Len(buf) < Capacity /\ nextVal <= MaxItems /\ buf' = Append(buf, nextVal) /\ nextVal' = nextVal + 1 /\ (UNCHANGED received)
Get == Len(buf) > 0 /\ received' = Append(received, Head(buf)) /\ buf' = Tail(buf) /\ (UNCHANGED nextVal)
Next == Put \/ Get
Spec == Init /\ ([][Next]_<<buf, nextVal, received>>)
NoOverflow == Len(buf) <= Capacity
FifoOrder == \A i \in 1..Len(received) : received[i] = i
NothingInvented == \A i \in 1..Len(buf) : buf[i] < nextVal
====

---- MODULE BankTransfer ----
EXTENDS Naturals, FiniteSets
CONSTANTS Acct, Amounts, InitBal
VARIABLES bal
Total == InitBal * Cardinality(Acct)
TypeOK == bal \in [Acct -> 0..Total]
Init == bal = [a \in Acct |-> InitBal]
Transfer(from, to, amt) == from # to /\ bal[from] >= amt /\ bal' = [bal EXCEPT ![from] = @ - amt, ![to] = @ + amt]
Next == \E f, t \in Acct, m \in Amounts : Transfer(f, t, m)
Spec == Init /\ ([][Next]_bal)
NoOverdraft == \A a \in Acct : bal[a] >= 0
RECURSIVE SumOver(_)
SumOver(S) == IF S = {} THEN 0 ELSE LET x == CHOOSE y \in S : TRUE IN bal[x] + SumOver(S \ {x})
Conserved == SumOver(Acct) = Total
====

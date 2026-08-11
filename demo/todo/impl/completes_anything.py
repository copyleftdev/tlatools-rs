"""The completion button does not check that the task is open.

A plausible thing to write, and a plausible thing for a language model to
write: the handler takes an id and marks it done.
"""

from correct import IDS, Todo as Correct


class Todo(Correct):
    def actions(self):
        out = []
        for i in IDS:
            if self.tasks[i] == "absent":
                out.append(("add", i))
            # No guard: complete is offered whatever state the task is in.
            out.append(("complete", i))
            if self.tasks[i] == "done":
                out.append(("reopen", i))
            if self.tasks[i] != "absent":
                out.append(("delete", i))
        if any(v == "done" for v in self.tasks.values()):
            out.append(("clear_completed", None))
        return out

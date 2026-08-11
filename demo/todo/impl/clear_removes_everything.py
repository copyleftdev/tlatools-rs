""""Clear completed" empties the whole list.

The guard is right -- it only offers the button when something is done -- but
the effect is wrong. This is the bug that survives a demo, because the button
looks like it works.
"""

from correct import IDS, Todo as Correct


class Todo(Correct):
    def step(self, action):
        kind, i = action
        if kind == "clear_completed":
            self.tasks = {k: "absent" for k in IDS}
        else:
            super().step(action)

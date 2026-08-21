"""Call-chain fixtures for the `tyf calls` command.

Every construct here exists to exercise one call-hierarchy behavior:
a 3-deep chain, a diamond (dedup), direct and mutual recursion, a method
chain through `self`, a decorated function in the chain, a stdlib call,
and a callback passed as an argument (invisible to static analysis).
"""

import functools
import math
from typing import Any, Callable


def audit(func: Callable[..., Any]) -> Callable[..., Any]:
    """Decorator used on `charge_payment` — exercises decorated callees."""

    @functools.wraps(func)
    def wrapper(*args: Any, **kwargs: Any) -> Any:
        return func(*args, **kwargs)

    return wrapper


# --- 3-deep chain + diamond -------------------------------------------------
# process_order -> {validate_order, charge_payment} -> check_inventory
# `check_inventory` is the diamond join point: reached via two paths, so it
# must be expanded once and marked as deduped on the second encounter.


def check_inventory(sku: str) -> int:
    """Leaf of the chain. Calls a stdlib function (external leaf)."""
    return math.floor(len(sku) * 1.5)


def validate_order(sku: str) -> bool:
    """Depth-1 callee; reaches `check_inventory` at depth 2."""
    return check_inventory(sku) > 0


@audit
def charge_payment(sku: str, amount: float) -> float:
    """Decorated depth-1 callee; also reaches `check_inventory`."""
    stock = check_inventory(sku)
    return amount * stock


def process_order(sku: str, amount: float) -> float:
    """Root of the chain: 3 levels deep, diamond over `check_inventory`."""
    if not validate_order(sku):
        return 0.0
    return charge_payment(sku, amount)


# --- recursion --------------------------------------------------------------


def countdown(n: int) -> int:
    """Direct recursion — a self-cycle in the call graph."""
    if n <= 0:
        return 0
    return countdown(n - 1)


def is_even(n: int) -> bool:
    """Mutual recursion: is_even -> is_odd -> is_even."""
    if n == 0:
        return True
    return is_odd(n - 1)


def is_odd(n: int) -> bool:
    """Mutual recursion: is_odd -> is_even -> is_odd."""
    if n == 0:
        return False
    return is_even(n - 1)


# --- method chain through `self` -------------------------------------------


class OrderPipeline:
    """Method chain: run -> prepare -> normalize, all through `self`."""

    def __init__(self, sku: str) -> None:
        self.sku = sku

    def normalize(self) -> str:
        """Innermost method in the `self` chain."""
        return self.sku.strip().upper()

    def prepare(self) -> str:
        """Calls `self.normalize()`."""
        return self.normalize()

    def run(self) -> float:
        """Calls `self.prepare()` and a module-level function."""
        sku = self.prepare()
        return process_order(sku, 10.0)


# --- callback passed as an argument (expected static-analysis miss) ---------


def apply_twice(fn: Callable[[int], int], value: int) -> int:
    """Invokes `fn` — the concrete target is not statically known here."""
    return fn(fn(value))


def double(value: int) -> int:
    """Passed to `apply_twice` as a callback, never called directly."""
    return value * 2


def run_callback_demo() -> int:
    """Passes `double` to `apply_twice`; the `double` call is indirect."""
    return apply_twice(double, 21)


# --- dedup must not hide a subtree that was inside the depth budget --------
# `budget_root` reaches `budget_shared` two ways: at level 2 via `budget_mid`
# (where a depth-2 walk has no budget left to expand it) and at level 1
# directly (where it does). Marking the level-1 occurrence as "already
# expanded above" would hide `budget_leaf`, which is legitimately 2 hops out.


def budget_leaf() -> int:
    """Two hops from `budget_root`; must appear in a depth-2 walk."""
    return 1


def budget_mid() -> int:
    """Reaches `budget_shared` at the depth-2 boundary.

    Defined *before* `budget_shared` on purpose: `ty` returns callees in
    definition order, so a walk from `budget_root` descends this branch first
    and meets `budget_shared` with no budget left.
    """
    return budget_shared()


def budget_shared() -> int:
    """Reached at two different depths from `budget_root`."""
    return budget_leaf()


def budget_root() -> int:
    """Reaches `budget_shared` at level 2 first, then again at level 1."""
    return budget_mid() + budget_shared()


# --- fan-out: many childless callees in one walk ---------------------------
# Each leaf has no outgoing calls. Retrying every empty result as if it were a
# cold-start miss would cost the full back-off ladder per leaf and blow the
# request timeout on this 12-line-deep tree.


def fan_leaf_0() -> int:
    return 0


def fan_leaf_1() -> int:
    return 1


def fan_leaf_2() -> int:
    return 2


def fan_leaf_3() -> int:
    return 3


def fan_leaf_4() -> int:
    return 4


def fan_leaf_5() -> int:
    return 5


def fan_leaf_6() -> int:
    return 6


def fan_leaf_7() -> int:
    return 7


def fan_out() -> int:
    """Eight callees, none of which call anything."""
    return (
        fan_leaf_0()
        + fan_leaf_1()
        + fan_leaf_2()
        + fan_leaf_3()
        + fan_leaf_4()
        + fan_leaf_5()
        + fan_leaf_6()
        + fan_leaf_7()
    )

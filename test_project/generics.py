"""Generics, TypeVar, type aliases, and advanced typing patterns."""

from dataclasses import dataclass, field
from typing import Generic, TypeVar, overload

T = TypeVar("T")
K = TypeVar("K")
V = TypeVar("V")
Numeric = TypeVar("Numeric", int, float)

# Type aliases
Pair = tuple[T, T]
Mapping = dict[str, list[T]]
Result = tuple[T, str | None]
Callback = "Callable[[T], None]"  # forward reference in string form


@dataclass
class Stack(Generic[T]):
    """Generic stack data structure."""

    _items: list[T] = field(default_factory=list)

    def push(self, item: T) -> None:
        self._items.append(item)

    def pop(self) -> T:
        if not self._items:
            raise IndexError("Stack is empty")
        return self._items.pop()

    def peek(self) -> T:
        if not self._items:
            raise IndexError("Stack is empty")
        return self._items[-1]

    @property
    def is_empty(self) -> bool:
        return len(self._items) == 0

    @property
    def size(self) -> int:
        return len(self._items)


@dataclass
class TreeNode(Generic[T]):
    """Generic binary tree node."""

    value: T
    left: "TreeNode[T] | None" = None
    right: "TreeNode[T] | None" = None

    def is_leaf(self) -> bool:
        return self.left is None and self.right is None

    def depth(self) -> int:
        left_depth = self.left.depth() if self.left else 0
        right_depth = self.right.depth() if self.right else 0
        return 1 + max(left_depth, right_depth)


class Registry(Generic[K, V]):
    """Multi-parameter generic class."""

    def __init__(self) -> None:
        self._store: dict[K, V] = {}

    def register(self, key: K, value: V) -> None:
        self._store[key] = value

    def lookup(self, key: K) -> V | None:
        return self._store.get(key)

    def all_keys(self) -> list[K]:
        return list(self._store.keys())

    def all_values(self) -> list[V]:
        return list(self._store.values())

    @property
    def count(self) -> int:
        return len(self._store)


@dataclass
class Page(Generic[T]):
    """Generic pagination container."""

    items: list[T]
    page_number: int
    page_size: int
    total_items: int

    @property
    def total_pages(self) -> int:
        return (self.total_items + self.page_size - 1) // self.page_size

    @property
    def has_next(self) -> bool:
        return self.page_number < self.total_pages

    @property
    def has_previous(self) -> bool:
        return self.page_number > 1


@overload
def first(items: list[T]) -> T: ...
@overload
def first(items: list[T], default: T) -> T: ...
def first(items: list[T], default: T | None = None) -> T | None:
    """Overloaded function: get first element or default."""
    if items:
        return items[0]
    return default


def clamp(value: Numeric, min_val: Numeric, max_val: Numeric) -> Numeric:
    """Function using constrained TypeVar."""
    if value < min_val:
        return min_val
    if value > max_val:
        return max_val
    return value


def merge_registries(a: Registry[K, V], b: Registry[K, V]) -> Registry[K, V]:
    """Function operating on generic types."""
    result: Registry[K, V] = Registry()
    for key in a.all_keys():
        val = a.lookup(key)
        if val is not None:
            result.register(key, val)
    for key in b.all_keys():
        val = b.lookup(key)
        if val is not None:
            result.register(key, val)
    return result


def paginate(items: list[T], page: int = 1, size: int = 10) -> Page[T]:
    """Generic pagination function."""
    start = (page - 1) * size
    end = start + size
    return Page(
        items=items[start:end],
        page_number=page,
        page_size=size,
        total_items=len(items),
    )

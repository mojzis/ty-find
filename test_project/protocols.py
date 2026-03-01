"""Abstract base classes, Protocols, and structural subtyping patterns."""

from abc import ABC, abstractmethod
from typing import Protocol, runtime_checkable


class Shape(ABC):
    """Abstract base class with abstract and concrete methods."""

    @abstractmethod
    def area(self) -> float:
        """Calculate area."""
        ...

    @abstractmethod
    def perimeter(self) -> float:
        """Calculate perimeter."""
        ...

    def describe(self) -> str:
        """Concrete method on ABC."""
        return f"{self.__class__.__name__}: area={self.area():.2f}"


class Circle(Shape):
    """Concrete implementation of Shape ABC."""

    PI: float = 3.14159265

    def __init__(self, radius: float) -> None:
        self.radius = radius

    def area(self) -> float:
        return self.PI * self.radius**2

    def perimeter(self) -> float:
        return 2 * self.PI * self.radius

    def diameter(self) -> float:
        return 2 * self.radius


class Rectangle(Shape):
    """Another concrete Shape implementation."""

    def __init__(self, width: float, height: float) -> None:
        self.width = width
        self.height = height

    def area(self) -> float:
        return self.width * self.height

    def perimeter(self) -> float:
        return 2 * (self.width + self.height)

    def is_square(self) -> bool:
        return self.width == self.height


@runtime_checkable
class Drawable(Protocol):
    """Protocol for drawable objects (structural subtyping)."""

    def draw(self, canvas: str) -> None: ...


@runtime_checkable
class Serializable(Protocol):
    """Protocol for serializable objects."""

    def to_dict(self) -> dict[str, object]: ...

    def to_json(self) -> str: ...


class Canvas:
    """Implements Drawable protocol implicitly (structural subtyping)."""

    def __init__(self, name: str) -> None:
        self.name = name
        self.elements: list[str] = []

    def draw(self, canvas: str) -> None:
        self.elements.append(f"drawn on {canvas}")


class Widget(ABC):
    """Multi-protocol ABC: abstract + concrete + properties."""

    _counter: int = 0

    def __init__(self, label: str) -> None:
        Widget._counter += 1
        self._id = Widget._counter
        self._label = label

    @property
    def widget_id(self) -> int:
        return self._id

    @property
    @abstractmethod
    def display_name(self) -> str:
        """Abstract property."""
        ...

    @abstractmethod
    def render(self) -> str: ...

    def to_dict(self) -> dict[str, object]:
        return {"id": self._id, "label": self._label}

    def to_json(self) -> str:
        import json
        return json.dumps(self.to_dict())


class Button(Widget):
    """Concrete widget implementing abstract property + method."""

    def __init__(self, label: str, action: str = "click") -> None:
        super().__init__(label)
        self.action = action

    @property
    def display_name(self) -> str:
        return f"[{self._label}]"

    def render(self) -> str:
        return f"<button>{self._label}</button>"

    def click(self) -> str:
        return f"Button '{self._label}' clicked: {self.action}"


class TextInput(Widget):
    """Widget with additional protocols."""

    def __init__(self, label: str, placeholder: str = "") -> None:
        super().__init__(label)
        self.placeholder = placeholder
        self.value: str = ""

    @property
    def display_name(self) -> str:
        return f"Input({self._label})"

    def render(self) -> str:
        return f'<input placeholder="{self.placeholder}"/>'

    def set_value(self, text: str) -> None:
        self.value = text


def calculate_total_area(shapes: list[Shape]) -> float:
    """Function using ABC type hint."""
    return sum(s.area() for s in shapes)


def render_widgets(widgets: list[Widget]) -> list[str]:
    """Function using ABC type hint for widget list."""
    return [w.render() for w in widgets]

"""Enum patterns: basic Enum, IntEnum, with methods and properties."""

from enum import Enum, IntEnum, auto


class Color(Enum):
    """Basic enum with explicit values."""

    RED = "red"
    GREEN = "green"
    BLUE = "blue"

    def hex_code(self) -> str:
        codes = {"red": "#FF0000", "green": "#00FF00", "blue": "#0000FF"}
        return codes[self.value]


class Priority(IntEnum):
    """Integer enum with auto values and comparison support."""

    LOW = auto()
    MEDIUM = auto()
    HIGH = auto()
    CRITICAL = auto()

    def label(self) -> str:
        return self.name.capitalize()

    @property
    def is_urgent(self) -> bool:
        return self >= Priority.HIGH


class HttpMethod(Enum):
    """Enum with class methods and custom behavior."""

    GET = "GET"
    POST = "POST"
    PUT = "PUT"
    DELETE = "DELETE"
    PATCH = "PATCH"

    @property
    def is_safe(self) -> bool:
        return self in (HttpMethod.GET,)

    @property
    def is_idempotent(self) -> bool:
        return self in (HttpMethod.GET, HttpMethod.PUT, HttpMethod.DELETE)

    @classmethod
    def from_string(cls, method: str) -> "HttpMethod":
        return cls(method.upper())


class TaskStatus(Enum):
    """Enum used as a state machine with transitions."""

    PENDING = "pending"
    IN_PROGRESS = "in_progress"
    REVIEW = "review"
    DONE = "done"
    CANCELLED = "cancelled"

    def can_transition_to(self, target: "TaskStatus") -> bool:
        transitions: dict[TaskStatus, list[TaskStatus]] = {
            TaskStatus.PENDING: [TaskStatus.IN_PROGRESS, TaskStatus.CANCELLED],
            TaskStatus.IN_PROGRESS: [TaskStatus.REVIEW, TaskStatus.CANCELLED],
            TaskStatus.REVIEW: [TaskStatus.DONE, TaskStatus.IN_PROGRESS],
            TaskStatus.DONE: [],
            TaskStatus.CANCELLED: [],
        }
        return target in transitions.get(self, [])

    @property
    def is_terminal(self) -> bool:
        return self in (TaskStatus.DONE, TaskStatus.CANCELLED)


class Permission(Enum):
    """Enum with bitwise-style combining via custom methods."""

    READ = 1
    WRITE = 2
    EXECUTE = 4
    ADMIN = 7  # READ | WRITE | EXECUTE

    def includes(self, other: "Permission") -> bool:
        return (self.value & other.value) == other.value


DEFAULT_COLOR: Color = Color.RED
DEFAULT_PRIORITY: Priority = Priority.MEDIUM


def format_task(name: str, status: TaskStatus, priority: Priority) -> str:
    """Function using multiple enum types."""
    urgent = " [URGENT]" if priority.is_urgent else ""
    return f"[{status.value}] {name} (P{priority.value}){urgent}"


def parse_permissions(value: int) -> list[Permission]:
    """Function returning enum members."""
    return [p for p in Permission if p.value & value]

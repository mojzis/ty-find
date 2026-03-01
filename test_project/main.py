"""Main module that imports from many other modules — for testing cross-file references."""

from models import Animal, Dog, Cat, create_dog, create_cat, MAX_ANIMALS
from decorators import greet, fetch_data, DatabaseConnection, MathService
from protocols import Circle, Rectangle, calculate_total_area, Button, TextInput
from enums import Color, Priority, TaskStatus, format_task
from generics import Stack, Registry, paginate
from patterns import Outer, make_multiplier, fibonacci, Timer, Point
from async_code import AsyncPool
from exceptions import AppError, ValidationError, validate_email
from typing_extras import Coordinate, RGB, Config, move


def list_animals(animals: list[Animal]) -> None:
    """Print all animals and their sounds."""
    for animal in animals:
        print(animal.speak())


def demo_models() -> None:
    """Demonstrate models module."""
    dog = create_dog("Rex")
    cat = create_cat("Whiskers")
    print(dog.fetch("ball"))
    print(cat.purr())
    animals: list[Animal] = [dog, cat]
    list_animals(animals)
    print(f"Max allowed: {MAX_ANIMALS}")


def demo_decorators() -> None:
    """Demonstrate decorator patterns."""
    print(greet("World"))
    data = fetch_data("https://example.com")
    print(data)
    db = DatabaseConnection("localhost", 5432)
    print(db.connect())
    math = MathService()
    print(math.expensive_computation(100))


def demo_protocols() -> None:
    """Demonstrate protocols and ABCs."""
    shapes = [Circle(5.0), Rectangle(3.0, 4.0)]
    total = calculate_total_area(shapes)
    print(f"Total area: {total}")
    btn = Button("Submit", "save")
    inp = TextInput("Name", "Enter name")
    print(btn.render())
    print(inp.render())


def demo_enums() -> None:
    """Demonstrate enum usage."""
    red = Color.RED
    print(red.hex_code())
    task = format_task("Deploy", TaskStatus.IN_PROGRESS, Priority.HIGH)
    print(task)


def demo_generics() -> None:
    """Demonstrate generic types."""
    stack: Stack[int] = Stack()
    stack.push(1)
    stack.push(2)
    print(stack.peek())
    reg: Registry[str, int] = Registry()
    reg.register("a", 1)
    page = paginate([1, 2, 3, 4, 5], page=1, size=2)
    print(page.total_pages)


def demo_patterns() -> None:
    """Demonstrate advanced patterns."""
    outer = Outer("test")
    inner = outer.create_inner(42)
    print(inner.double())
    double = make_multiplier(2)
    print(double(21))
    fibs = list(fibonacci(100))
    print(fibs)
    with Timer("demo") as t:
        pass
    print(t.elapsed)
    p = Point(1.0, 2.0)
    q = Point(4.0, 6.0)
    print(p.distance_to(q))


def demo_typing() -> None:
    """Demonstrate typing extras."""
    origin = Coordinate(0.0, 0.0)
    pt = Coordinate(3.0, 4.0)
    print(origin.distance_to(pt))
    white = RGB(255, 255, 255)
    print(white.hex())
    cfg = Config("app", debug=True)
    print(cfg.to_dict())
    print(move("north", 5))


def demo_exceptions() -> None:
    """Demonstrate exception patterns."""
    try:
        validate_email("bad")
    except ValidationError as e:
        print(e.error_id)


def main() -> None:
    """Entry point — exercises all modules."""
    demo_models()
    demo_decorators()
    demo_protocols()
    demo_enums()
    demo_generics()
    demo_patterns()
    demo_typing()
    demo_exceptions()


if __name__ == "__main__":
    main()

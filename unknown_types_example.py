"""Test fixture for Unknown-type substitution (members/show).

The import below is deliberately unresolvable, so `ty` cannot resolve the
annotations that reference `Widget` and widens them to `Unknown`. The CLI must
substitute the literal source annotation instead. This simulates the real case
where a third-party library's stubs are not installed in the analyzed
environment (e.g. `pyarrow`).
"""

from nonexistent_pkg import Widget  # type: ignore


class Workshop:
    """A class whose annotations reference an unresolvable type."""

    # Class variable annotated with an unresolvable type.
    default_widget: Widget = make_default()  # noqa: F821

    def build(self) -> Widget:
        """Return type is unresolvable -> ty reports Unknown."""
        return Widget()

    def build_many(self) -> list[Widget]:
        """Generic over an unresolvable type."""
        return []

    def build_with_options(
        self,
        name: str,
        count: int = 1,
    ) -> Widget:
        """Multi-line signature with an unresolvable return type."""
        return Widget()

    def build_lazy(self) -> "Widget":
        """Stringized annotation referencing an unresolvable type."""
        return Widget()

    def resolved(self) -> str:
        """Normally-resolvable return type — must be unchanged."""
        return "ok"

"""Async patterns: async functions, async generators, async context managers."""

from typing import Any, AsyncGenerator, AsyncIterator


async def fetch_url(url: str, timeout: float = 10.0) -> str:
    """Basic async function."""
    return f"Response from {url}"


async def fetch_all(urls: list[str]) -> list[str]:
    """Async function calling other async functions."""
    results: list[str] = []
    for url in urls:
        result = await fetch_url(url)
        results.append(result)
    return results


async def process_item(item: dict[str, Any]) -> dict[str, Any]:
    """Async function with complex types."""
    return {**item, "processed": True}


async def stream_items(count: int) -> AsyncGenerator[int, None]:
    """Async generator function."""
    for i in range(count):
        yield i


async def filtered_stream(
    source: AsyncGenerator[int, None], predicate: "Callable[[int], bool]"
) -> AsyncGenerator[int, None]:
    """Async generator consuming another async generator."""
    async for item in source:
        if predicate(item):
            yield item


class AsyncResource:
    """Async context manager class."""

    def __init__(self, name: str) -> None:
        self.name = name
        self.is_open = False

    async def __aenter__(self) -> "AsyncResource":
        self.is_open = True
        return self

    async def __aexit__(self, *args: Any) -> None:
        self.is_open = False

    async def read(self) -> str:
        if not self.is_open:
            raise RuntimeError("Resource not open")
        return f"data from {self.name}"


class AsyncPool:
    """Async class with multiple async methods."""

    def __init__(self, size: int = 10) -> None:
        self.size = size
        self._connections: list[str] = []

    async def acquire(self) -> str:
        connection = f"conn-{len(self._connections)}"
        self._connections.append(connection)
        return connection

    async def release(self, connection: str) -> None:
        if connection in self._connections:
            self._connections.remove(connection)

    async def execute(self, query: str) -> list[dict[str, Any]]:
        conn = await self.acquire()
        result = [{"query": query, "connection": conn}]
        await self.release(conn)
        return result

    @property
    def active_count(self) -> int:
        return len(self._connections)

    async def close_all(self) -> int:
        count = len(self._connections)
        self._connections.clear()
        return count


class AsyncIterableRange:
    """Class implementing async iterator protocol."""

    def __init__(self, start: int, stop: int) -> None:
        self.start = start
        self.stop = stop

    def __aiter__(self) -> AsyncIterator[int]:
        self._current = self.start
        return self  # type: ignore[return-value]

    async def __anext__(self) -> int:
        if self._current >= self.stop:
            raise StopAsyncIteration
        value = self._current
        self._current += 1
        return value


DEFAULT_POOL_SIZE: int = 10
CONNECTION_TIMEOUT: float = 30.0

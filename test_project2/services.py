"""Service layer for test_project2."""


class User:
    """A user in the system."""

    def __init__(self, name: str, email: str) -> None:
        self.name = name
        self.email = email

    def display_name(self) -> str:
        """Return formatted display name."""
        return f"{self.name} <{self.email}>"


class UserService:
    """Manages user operations."""

    def __init__(self) -> None:
        self._users: list[User] = []

    def add_user(self, name: str, email: str) -> User:
        """Create and add a new user."""
        user = User(name, email)
        self._users.append(user)
        return user

    def list_users(self) -> list[User]:
        """Return all registered users."""
        return list(self._users)

    def find_user(self, name: str) -> User | None:
        """Find a user by name."""
        for user in self._users:
            if user.name == name:
                return user
        return None


def format_greeting(user: User) -> str:
    """Format a greeting message for a user."""
    return f"Hello, {user.display_name()}!"

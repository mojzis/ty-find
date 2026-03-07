"""Application module for test_project2 — exercises cross-workspace daemon resolution."""

from services import UserService, format_greeting


def run_app() -> None:
    """Main entry point for the application."""
    svc = UserService()
    svc.add_user("Alice", "alice@example.com")
    svc.add_user("Bob", "bob@example.com")
    users = svc.list_users()
    for user in users:
        print(format_greeting(user))


if __name__ == "__main__":
    run_app()

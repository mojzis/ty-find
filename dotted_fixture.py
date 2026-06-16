"""Fixture for one-level dotted-notation tests (`Class.member`).

Two distinct classes each define a method named ``get_data`` so tests can
verify that ``Database.get_data`` resolves to *only* ``Database``'s member and
not ``Cache``'s same-named method.
"""


class Database:
    def get_data(self):
        return "database rows"

    def connect(self):
        return True


class Cache:
    def get_data(self):
        return "cached value"

    def evict(self):
        return None


def use_database():
    db = Database()
    return db.get_data()

def hello_world():
    return "Hello, World!"


def calculate_sum(a, b):
    return a + b


class Calculator:
    def add(self, a, b):
        return a + b

    def multiply(self, a, b):
        return a * b


def main():
    result = hello_world()
    total = calculate_sum(1, 2)
    calc = Calculator()
    product = calc.multiply(3, 4)
    print(f"{result} - Sum: {total} - Product: {product}")


if __name__ == "__main__":
    main()

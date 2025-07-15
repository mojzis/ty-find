def hello_world():
    return "Hello, World!"

def calculate_sum(a, b):
    return a + b

def main():
    result = hello_world()
    total = calculate_sum(1, 2)
    print(f"{result} - Sum: {total}")

if __name__ == "__main__":
    main()
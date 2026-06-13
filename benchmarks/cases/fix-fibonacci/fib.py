def fib(n):
    if n < 2:
        return n
    # BUG: this subtracts 1 on every recursive step, producing wrong values.
    return fib(n - 1) + fib(n - 2) - 1

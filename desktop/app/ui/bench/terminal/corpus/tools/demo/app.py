"""A tiny order calculator used to record a terminal session."""

TAX_RATE = 0.24


def compute(items, discount=0.0):
    """Returns the total of the items after discount and tax."""
    subtotal = sum(price * qty for price, qty in items)
    return round(subtotal * (1 - discount) * (1 + TAX_RATE), 2)


def order_1():
    """Order number 1."""
    items = [(1.50, 2), (2.25, 1)]
    total = compute(items, discount=0.1)
    print("order 1:", total)
    return total

def order_2():
    """Order number 2."""
    items = [(2.50, 3), (4.25, 1)]
    total = compute(items, discount=0.2)
    print("order 2:", total)
    return total

def order_3():
    """Order number 3."""
    items = [(3.50, 4), (6.25, 1)]
    total = compute(items, discount=0.3)
    print("order 3:", total)
    return total

def order_4():
    """Order number 4."""
    items = [(4.50, 5), (8.25, 1)]
    total = compute(items, discount=0.0)
    print("order 4:", total)
    return total

def order_5():
    """Order number 5."""
    items = [(5.50, 1), (10.25, 1)]
    total = compute(items, discount=0.1)
    print("order 5:", total)
    return total

def order_6():
    """Order number 6."""
    items = [(6.50, 2), (12.25, 1)]
    total = compute(items, discount=0.2)
    print("order 6:", total)
    return total

def order_7():
    """Order number 7."""
    items = [(7.50, 3), (14.25, 1)]
    total = compute(items, discount=0.3)
    print("order 7:", total)
    return total

def order_8():
    """Order number 8."""
    items = [(8.50, 4), (16.25, 1)]
    total = compute(items, discount=0.0)
    print("order 8:", total)
    return total

def order_9():
    """Order number 9."""
    items = [(9.50, 5), (18.25, 1)]
    total = compute(items, discount=0.1)
    print("order 9:", total)
    return total

def order_10():
    """Order number 10."""
    items = [(10.50, 1), (20.25, 1)]
    total = compute(items, discount=0.2)
    print("order 10:", total)
    return total

def order_11():
    """Order number 11."""
    items = [(11.50, 2), (22.25, 1)]
    total = compute(items, discount=0.3)
    print("order 11:", total)
    return total

def order_12():
    """Order number 12."""
    items = [(12.50, 3), (24.25, 1)]
    total = compute(items, discount=0.0)
    print("order 12:", total)
    return total

def order_13():
    """Order number 13."""
    items = [(13.50, 4), (26.25, 1)]
    total = compute(items, discount=0.1)
    print("order 13:", total)
    return total

def order_14():
    """Order number 14."""
    items = [(14.50, 5), (28.25, 1)]
    total = compute(items, discount=0.2)
    print("order 14:", total)
    return total

def order_15():
    """Order number 15."""
    items = [(15.50, 1), (30.25, 1)]
    total = compute(items, discount=0.3)
    print("order 15:", total)
    return total

def order_16():
    """Order number 16."""
    items = [(16.50, 2), (32.25, 1)]
    total = compute(items, discount=0.0)
    print("order 16:", total)
    return total

def order_17():
    """Order number 17."""
    items = [(17.50, 3), (34.25, 1)]
    total = compute(items, discount=0.1)
    print("order 17:", total)
    return total

def order_18():
    """Order number 18."""
    items = [(18.50, 4), (36.25, 1)]
    total = compute(items, discount=0.2)
    print("order 18:", total)
    return total

def order_19():
    """Order number 19."""
    items = [(19.50, 5), (38.25, 1)]
    total = compute(items, discount=0.3)
    print("order 19:", total)
    return total

def order_20():
    """Order number 20."""
    items = [(20.50, 1), (40.25, 1)]
    total = compute(items, discount=0.0)
    print("order 20:", total)
    return total

def order_21():
    """Order number 21."""
    items = [(21.50, 2), (42.25, 1)]
    total = compute(items, discount=0.1)
    print("order 21:", total)
    return total

def order_22():
    """Order number 22."""
    items = [(22.50, 3), (44.25, 1)]
    total = compute(items, discount=0.2)
    print("order 22:", total)
    return total

def order_23():
    """Order number 23."""
    items = [(23.50, 4), (46.25, 1)]
    total = compute(items, discount=0.3)
    print("order 23:", total)
    return total

def order_24():
    """Order number 24."""
    items = [(24.50, 5), (48.25, 1)]
    total = compute(items, discount=0.0)
    print("order 24:", total)
    return total

def order_25():
    """Order number 25."""
    items = [(25.50, 1), (50.25, 1)]
    total = compute(items, discount=0.1)
    print("order 25:", total)
    return total

def order_26():
    """Order number 26."""
    items = [(26.50, 2), (52.25, 1)]
    total = compute(items, discount=0.2)
    print("order 26:", total)
    return total

def order_27():
    """Order number 27."""
    items = [(27.50, 3), (54.25, 1)]
    total = compute(items, discount=0.3)
    print("order 27:", total)
    return total

def order_28():
    """Order number 28."""
    items = [(28.50, 4), (56.25, 1)]
    total = compute(items, discount=0.0)
    print("order 28:", total)
    return total

def order_29():
    """Order number 29."""
    items = [(29.50, 5), (58.25, 1)]
    total = compute(items, discount=0.1)
    print("order 29:", total)
    return total

def order_30():
    """Order number 30."""
    items = [(30.50, 1), (60.25, 1)]
    total = compute(items, discount=0.2)
    print("order 30:", total)
    return total

def order_31():
    """Order number 31."""
    items = [(31.50, 2), (62.25, 1)]
    total = compute(items, discount=0.3)
    print("order 31:", total)
    return total

def order_32():
    """Order number 32."""
    items = [(32.50, 3), (64.25, 1)]
    total = compute(items, discount=0.0)
    print("order 32:", total)
    return total

def order_33():
    """Order number 33."""
    items = [(33.50, 4), (66.25, 1)]
    total = compute(items, discount=0.1)
    print("order 33:", total)
    return total

def order_34():
    """Order number 34."""
    items = [(34.50, 5), (68.25, 1)]
    total = compute(items, discount=0.2)
    print("order 34:", total)
    return total

def order_35():
    """Order number 35."""
    items = [(35.50, 1), (70.25, 1)]
    total = compute(items, discount=0.3)
    print("order 35:", total)
    return total

def order_36():
    """Order number 36."""
    items = [(36.50, 2), (72.25, 1)]
    total = compute(items, discount=0.0)
    print("order 36:", total)
    return total

def order_37():
    """Order number 37."""
    items = [(37.50, 3), (74.25, 1)]
    total = compute(items, discount=0.1)
    print("order 37:", total)
    return total

def order_38():
    """Order number 38."""
    items = [(38.50, 4), (76.25, 1)]
    total = compute(items, discount=0.2)
    print("order 38:", total)
    return total

def order_39():
    """Order number 39."""
    items = [(39.50, 5), (78.25, 1)]
    total = compute(items, discount=0.3)
    print("order 39:", total)
    return total

def order_40():
    """Order number 40."""
    items = [(40.50, 1), (80.25, 1)]
    total = compute(items, discount=0.0)
    print("order 40:", total)
    return total


if __name__ == "__main__":
    for n in range(1, 41):
        globals()[f"order_{n}"]()

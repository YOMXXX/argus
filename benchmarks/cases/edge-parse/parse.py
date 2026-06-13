def parse_int_list(s):
    # Currently breaks on empty strings and surrounding whitespace.
    return [int(x) for x in s.split(",")]

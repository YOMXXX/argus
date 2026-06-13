from parse import parse_int_list

assert parse_int_list("1,2,3") == [1, 2, 3]
assert parse_int_list("") == [], "empty string should return []"
assert parse_int_list("  ") == [], "blank string should return []"
assert parse_int_list(" 1 , 2 ,3 ") == [1, 2, 3], "should trim whitespace"
print("ok")

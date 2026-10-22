"""Drives granary's hash equivalence server with BitBake's client.

Usage: hashserv.py <bitbake root> <main url> <pr url> <main token> <pr token>
"""

import sys

sys.path.insert(0, f"{sys.argv[1]}/lib")

import hashserv  # noqa: E402
from bb.asyncrpc.exceptions import InvokeError  # noqa: E402

main_url, pr_url, main_token, pr_token = sys.argv[2:6]
METHOD = "oe.sstatesig.OEOuthashBasic"
T1, T2, T3 = ("1" * 64, "2" * 64, "3" * 64)
OUT = "f" * 64
U1, U2, U3 = ("a" * 64, "b" * 64, "c" * 64)


def check(condition, reason):
    if not condition:
        raise AssertionError(reason)


anonymous = hashserv.create_client(main_url)
check(anonymous.ping() == {"alive": True}, "ping")
check(anonymous.get_unihash(METHOD, T1) is None, "unknown tasks have no unihash")
report = anonymous.report_unihash(T1, METHOD, OUT, U1)
check(report["unihash"] == U1, "an anonymous report answers its own unihash")
check(anonymous.get_unihash(METHOD, T1) is None, "an anonymous report isn't recorded")
try:
    anonymous.report_unihash_equiv(T1, METHOD, U1)
    check(False, "anonymous equivalence reports are refused")
except InvokeError:
    pass

main = hashserv.create_client(main_url, "ci", main_token)
check(main.report_unihash(T1, METHOD, OUT, U1)["unihash"] == U1, "the first report wins")
check(
    main.report_unihash(T2, METHOD, OUT, U2)["unihash"] == U1,
    "an equal output hash adopts the first unihash",
)
check(
    main.get_unihash_batch([(METHOD, T1), (METHOD, T2), (METHOD, T3)]) == [U1, U1, None],
    "batched lookups keep their order",
)
check(main.unihash_exists_batch([U1, U3]) == [True, False], "existence checks")
check(
    main.get_unihash_batch([(METHOD, "xyz"), (METHOD, T1)]) == [None, U1],
    "malformed task hashes are misses",
)
check(
    main.unihash_exists_batch(["XYZ", U1]) == [False, True],
    "malformed unihashes don't exist",
)
full = main.get_taskhash(METHOD, T1, True)
check(full["outhash"] == OUT and full["unihash"] == U1, "full task lookups")
check(full["owner"] is not None, "logged in readers see the reporter")
check(anonymous.get_taskhash(METHOD, T1, True)["owner"] is None, "anonymous readers don't")
check(main.get_outhash(METHOD, OUT, T1)["unihash"] == U1, "output hash lookups")
check(
    main.report_unihash_equiv(T3, METHOD, U3)["unihash"] == U3,
    "equivalence reports",
)
siginfo = "0" * (256 * 1024)
main.report_unihash("5" * 64, METHOD, "e" * 64, "5" * 64, {"outhash_siginfo": siginfo})
check(
    main.get_taskhash(METHOD, "5" * 64, True)["outhash_siginfo"] == siginfo,
    "large signature data",
)

for read in (lambda c: c.get_unihash(METHOD, T1), lambda c: c.get_stats()):
    try:
        read(hashserv.create_client(pr_url))
        check(False, "private caches need a token")
    except InvokeError:
        pass

pr = hashserv.create_client(pr_url, "ci", pr_token)
check(pr.get_unihash(METHOD, T1) == U1, "the fallback answers reads")
check(
    pr.report_unihash("4" * 64, METHOD, OUT, "d" * 64)["unihash"] == U1,
    "reports adopt unihashes from the fallback",
)
check(main.get_unihash(METHOD, "4" * 64) is None, "reports stay in their cache")
try:
    hashserv.create_client(main_url, "ci", "invalid").ping()
    check(False, "invalid tokens are refused")
except InvokeError:
    pass

stats = main.get_stats()["requests"]
check(stats["num"] > 0 and stats["max_time"] >= stats["average"] > 0, f"statistics: {stats}")
check(main.reset_stats()["requests"]["num"] == stats["num"] + 1, "reset answers the old statistics")
check(main.get_stats()["requests"]["num"] == 1, "reset starts over")
try:
    anonymous.reset_stats()
    check(False, "anonymous clients can't reset statistics")
except InvokeError:
    pass

print("ok")

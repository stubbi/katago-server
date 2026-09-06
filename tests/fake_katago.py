#!/usr/bin/env python3
"""A stand-in for `katago analysis` that speaks the same JSON line protocol.

Used by the integration tests so the server can be exercised end to end without
a neural network. Behaviour is steered through `overrideSettings` keys that real
KataGo would never see:

  fakeDelayMs   sleep before answering each analysed turn (drives timeout tests)
  fakeCrash     exit immediately with status 7 (drives restart tests)
  fakeWarn      emit a warning line before the result
  fakeError     emit an error without a field (an "engine" error)

Environment:
  FAKE_KATAGO_LOG            append every received query line to this file
  FAKE_KATAGO_STARTUP_DELAY  seconds to sleep before reading stdin
  FAKE_KATAGO_CRASH_ON_START exit with status 3 before reading anything
"""
import json
import os
import sys
import time

KNOWN_RULES = {
    "chinese", "japanese", "korean", "aga", "tromp-taylor", "bga", "new-zealand",
    "stone-scoring", "chinese-ogs", "chinese-kgs", "aga-button", "ancient-area",
    "ancient-territory",
}


def emit(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def log_query(line):
    path = os.environ.get("FAKE_KATAGO_LOG")
    if path:
        with open(path, "a", encoding="utf-8") as f:
            f.write(line + "\n")


def analysis(q):
    qid = q.get("id")
    delay = (q.get("overrideSettings") or {}).get("fakeDelayMs", 0) / 1000.0
    moves = q.get("moves", [])
    seen = set()
    for i, (_, coord) in enumerate(moves):
        if coord.lower() != "pass":
            if coord in seen:
                emit({"error": f"Illegal move {i}: {coord}", "field": "moves", "id": qid})
                return
            seen.add(coord)
    rules = q.get("rules")
    if isinstance(rules, str) and rules not in KNOWN_RULES:
        emit({"error": f"Could not parse rules: {rules}", "field": "rules", "id": qid})
        return

    x, y = q["boardXSize"], q["boardYSize"]
    visits = q.get("maxVisits", 10)
    first = (q.get("initialPlayer") or ("W" if q.get("initialStones") else "B")).upper()
    for turn in q.get("analyzeTurns", [len(moves)]):
        if delay:
            time.sleep(delay)
        if turn < len(moves):
            player = moves[turn][0].upper()
        else:
            player = first if turn % 2 == 0 else ("W" if first == "B" else "B")
        best = "Q16" if x >= 16 and y >= 16 else "C3"
        move_info = {
            "move": best, "visits": visits, "winrate": 0.5, "scoreMean": 0.1,
            "scoreStdev": 10.0, "scoreLead": 0.1, "scoreSelfplay": 0.2, "utility": 0.0,
            "utilityLcb": -0.1, "lcb": 0.45, "prior": 0.3, "order": 0, "pv": [best],
            "edgeVisits": visits, "edgeWeight": 1.0, "weight": 1.0, "playSelectionValue": 1.0,
        }
        if q.get("includePVVisits"):
            move_info["pvVisits"] = [visits]
            move_info["pvEdgeVisits"] = [visits]
        if q.get("includeMovesOwnership"):
            move_info["ownership"] = [0.0] * (x * y)
        resp = {
            "id": qid, "isDuringSearch": False, "turnNumber": turn,
            "moveInfos": [move_info],
            "rootInfo": {
                "winrate": 0.5, "scoreLead": 0.1, "scoreSelfplay": 0.2, "scoreStdev": 10.0,
                "utility": 0.0, "visits": visits, "currentPlayer": player, "weight": 1.0,
                "rawWinrate": 0.5, "rawLead": 0.1, "symHash": "00", "thisHash": "00",
            },
        }
        if q.get("includeOwnership"):
            resp["ownership"] = [0.0] * (x * y)
        if q.get("includeOwnershipStdev"):
            resp["ownershipStdev"] = [0.1] * (x * y)
        if q.get("includePolicy"):
            resp["policy"] = [1.0 / (x * y + 1)] * (x * y + 1)
        emit(resp)


def main():
    time.sleep(float(os.environ.get("FAKE_KATAGO_STARTUP_DELAY", "0")))
    sys.stderr.write("fake katago: loading pretend model\n")
    sys.stderr.flush()
    if os.environ.get("FAKE_KATAGO_CRASH_ON_START"):
        sys.stderr.write("fake katago: crashing on start\n")
        sys.exit(3)
    for raw in sys.stdin:
        line = raw.strip()
        if not line:
            continue
        log_query(line)
        try:
            q = json.loads(line)
        except ValueError:
            emit({"error": "could not parse input line as json request: " + line})
            continue
        qid = q.get("id")
        action = q.get("action")
        if action == "query_version":
            emit({"action": action, "git_hash": "fakehash123", "id": qid, "version": "9.9.9-fake"})
        elif action in ("clear_cache", "terminate_all"):
            emit({"action": action, "id": qid})
        elif action == "terminate":
            emit({"action": action, "id": qid, "terminateId": q.get("terminateId")})
        elif action:
            emit({"error": f"unknown action {action}", "id": qid})
        else:
            overrides = q.get("overrideSettings") or {}
            if "fakeCrash" in overrides:
                sys.stderr.write("fake katago: crashing on purpose\n")
                sys.stderr.flush()
                os._exit(7)
            if "fakeWarn" in overrides:
                emit({"field": "fakeWarn", "id": qid, "warning": "Unexpected or unused field"})
            if "fakeError" in overrides:
                emit({"error": overrides["fakeError"], "id": qid})
                continue
            analysis(q)


if __name__ == "__main__":
    main()

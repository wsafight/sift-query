import assert from "node:assert/strict";
import test from "node:test";
import { createQueryBuilder, encodeQuery, search } from "./sift-query.ts";

test("canonical encoding preserves ordered repeats, Unicode, plus and delimiters", () => {
  assert.equal(encodeQuery([["name", "中文 +&="], ["name", "two"]]),
    "name=%E4%B8%AD%E6%96%87%20%2B%26%3D&name=two");
  assert.equal(encodeQuery([["chars", "~*!'()"]]), "chars=~%2A%21%27%28%29");
});

test("GET and POST send identical parameters and preserve caller request settings", async () => {
  const calls: [string, RequestInit][] = [];
  const fetcher = (async (input: URL | RequestInfo, init?: RequestInit) => {
    calls.push([String(input), init ?? {}]);
    return new Response("{}");
  }) as typeof fetch;
  const entries = [["id", "2"], ["id", "1"], ["id-op", "il"]] as const;
  await search("https://example.test/users", entries, { fetcher, credentials: "include" });
  await search("https://example.test/users", entries, { fetcher, getUrlBytes: 1, credentials: "include" });
  assert.equal(calls[0][1].method, "GET");
  assert.equal(calls[1][1].method, "POST");
  assert.equal(new URL(calls[0][0]).search.slice(1), calls[1][1].body);
  assert.equal(new URL(calls[1][0]).search, "");
  assert.equal(calls[1][1].credentials, "include");
  assert.equal(new Headers(calls[1][1].headers).get("content-type"), "application/x-www-form-urlencoded;charset=UTF-8");
});

test("full encoded URL determines transport and budget failures never reach fetch", async () => {
  let calls = 0;
  const fetcher = (async () => { calls++; return new Response(null, { status: 414 }); }) as typeof fetch;
  const response = await search("https://example.test/very-long-prefix", [["name", "中"]], { fetcher, getUrlBytes: 40 });
  assert.equal(response.status, 414);
  assert.equal(calls, 1);
  await assert.rejects(search("https://example.test/users", [["name", "中"]], { fetcher, maxQueryBytes: 10 }), RangeError);
  await assert.rejects(search("https://example.test/users", [["id", 1], ["id", 2]], { fetcher, maxRawItems: 1 }), RangeError);
  await assert.rejects(search("https://example.test/users?id=1", [], { fetcher }), TypeError);
  assert.equal(calls, 1);
});

test("typed query builder emits the RawParams protocol and validates metadata", () => {
  const query = createQueryBuilder({
    fields: {
      id: { filterable: true, selectable: true, sortable: true, operators: ["eq", "il"] },
      name: { filterable: true, selectable: true, sortable: true, operators: ["eq", "ct"] },
    },
  })
    .where("name", "Ada")
    .whereIn("id", [2, 1])
    .onlySelect("id", "name")
    .orderBy("id:asc")
    .size(20)
    .cursor("");
  assert.equal(
    query.toQueryString(),
    "name=Ada&id=2&id=1&id-op=il&onlySelect=id%2Cname&orderBy=id%3Aasc&size=20&cursor=",
  );
  assert.throws(() => query.onlySelect("missing" as "id"), RangeError);
  assert.throws(() => query.size(0), RangeError);
});

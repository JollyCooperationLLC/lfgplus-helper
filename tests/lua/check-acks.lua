-- Loads an acks file the helper wrote, as the game loads an add-on's file, and checks what LFG+ will find in it.
--
--   lua5.1 tests/lua/check-acks.lua tests/vectors/Acks.lua
--
-- The vector is the helper's own output for these acks (src/acks.rs checks that it still is).

local expected = {
    ["a1b2c3d4e5f60718"] = 812,
    ["ffffffff00000000"] = 3,
}

local chunk = assert(loadfile(arg[1]))
local globals = {}
setfenv(chunk, globals)
chunk()

local acks = globals.LFGPlusHelperAcks
assert(type(acks) == "table", "LFGPlusHelperAcks isn't a table")
for install, seq in pairs(acks) do
    assert(type(install) == "string" and #install == 16 and install:match("^%x+$"), "odd install id " .. tostring(install))
    assert(type(seq) == "number" and seq == math.floor(seq), "odd number for " .. install)
    assert(expected[install] == seq, "unexpected ack for " .. install)
end
for install, seq in pairs(expected) do
    assert(acks[install] == seq, "missing ack for " .. install)
end
for name in pairs(globals) do
    assert(name == "LFGPlusHelperAcks", "the file sets another global: " .. name)
end
print("ok")

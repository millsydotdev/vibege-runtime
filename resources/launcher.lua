-- VibeGE Game Store Launcher
-- Embedded game store with Live/Dev channels and keyboard navigation.
-- Uses the bitmap font renderer for all text.

local sw, sh = 800, 600
local games_live = {}
local games_dev = {}
local channel = "live" -- "live" or "dev"
local selection = 1
local frame_count = 0
local key_cooldown = 0

-- Colors
local COL_BG       = {0.05, 0.05, 0.15}
local COL_ACCENT   = {0.48, 0.23, 0.93}
local COL_ACCENT2  = {0.38, 0.13, 0.83}
local COL_CARD     = {0.10, 0.10, 0.22}
local COL_CARD_HL  = {0.18, 0.18, 0.35}
local COL_CARD_SEL = {0.25, 0.15, 0.45}
local COL_WHITE    = {1.0,  1.0,  1.0}
local COL_DIM      = {0.5,  0.5,  0.6}
local COL_GREEN    = {0.2,  0.8,  0.4}
local COL_YELLOW   = {0.9,  0.7,  0.2}
local COL_RED      = {0.9,  0.3,  0.3}

local MARGIN = 30
local CARD_H = 72
local GAP = 8

-- Helper: draw text with standard size
local function text(x, y, str, size, cr, cg, cb)
    if str and #str > 0 then
        vibege.render.draw_text(x, y, str, size, cr, cg, cb)
    end
end

-- Helper: draw colored rectangle
local function rect(x, y, w, h, cr, cg, cb, ca)
    vibege.render.draw_rect(x, y, w, h, cr, cg, cb, ca)
end

-- Helper: check if a key was just pressed (with cooldown to prevent double-fire)
local function pressed(key)
    if vibege.input.is_key_pressed(key) then
        if key_cooldown <= 0 then
            key_cooldown = 6  -- frames to wait
            return true
        end
    end
    return false
end

-- Sample games
local function init_games()
    games_live = {
        {name="Pong",          desc="Classic paddle arcade",             author="VibeGE",         status="live", size="1.2 MB", plays=1240},
        {name="Asteroids",     desc="Shoot rocks in space",             author="VibeGE",         status="live", size="2.1 MB", plays=892},
        {name="Snake",         desc="Grow the longest snake",           author="Community",      status="live", size="0.5 MB", plays=456},
    }
    games_dev = {
        {name="Void Drifter",  desc="Space exploration survival",       author="VibeGE Labs",    status="dev",  size="3.4 MB", plays=67},
        {name="Block Puzzle",  desc="Relaxing puzzle game",            author="Community Dev",  status="dev",  size="1.8 MB", plays=23},
    }
end

function init()
    init_games()
    math.randomseed(os.time())
    print("VibeGE Launcher started")
end

function update(dt)
    if key_cooldown > 0 then key_cooldown = key_cooldown - 1 end
    frame_count = frame_count + 1

    -- Channel switching
    if pressed("tab") then
        if channel == "live" then channel = "dev" else channel = "live" end
        selection = 1
    end

    -- Navigation
    local active = games_live
    if channel == "dev" then active = games_dev end

    if pressed("up") and selection > 1 then selection = selection - 1 end
    if pressed("down") and selection < #active then selection = selection + 1 end

    -- Launch
    if pressed("enter") then
        local game = active[selection]
        if game then
            print("Launching: " .. game.name)
        end
    end

    -- Escape quits
    if vibege.input.is_key_down("escape") then
        error("exit", 0)
    end
end

local function draw_tab(x, y, w, h, label, is_active)
    if is_active then
        rect(x, y, w, h, COL_ACCENT[1], COL_ACCENT[2], COL_ACCENT[3], 1)
    else
        rect(x, y, w, h, COL_CARD[1], COL_CARD[2], COL_CARD[3], 1)
    end
    text(x + w/2 - #label * 4, y + 7, label, 8, COL_WHITE[1], COL_WHITE[2], COL_WHITE[3])
end

local function draw_game_card(x, y, w, game, is_selected)
    -- Card background
    if is_selected then
        rect(x, y, w, CARD_H, COL_CARD_SEL[1], COL_CARD_SEL[2], COL_CARD_SEL[3], 1)
        -- Selection border
        rect(x, y, 3, CARD_H, COL_ACCENT[1], COL_ACCENT[2], COL_ACCENT[3], 1)
    else
        rect(x, y, w, CARD_H, COL_CARD[1], COL_CARD[2], COL_CARD[3], 1)
    end

    -- Game name (larger text)
    text(x + 16, y + 8, game.name, 10, COL_WHITE[1], COL_WHITE[2], COL_WHITE[3])

    -- Description
    text(x + 16, y + 26, game.desc, 8, COL_DIM[1], COL_DIM[2], COL_DIM[3])

    -- Author
    text(x + 16, y + 42, "by " .. game.author, 7, COL_DIM[1], COL_DIM[2], COL_DIM[3])

    -- Status badge
    local sx = x + w - 85
    local sc
    if game.status == "live" then
        sc = COL_GREEN
        rect(sx, y + 8, 70, 16, COL_GREEN[1], COL_GREEN[2], COL_GREEN[3], 0.2)
    else
        sc = COL_YELLOW
        rect(sx, y + 8, 70, 16, COL_YELLOW[1], COL_YELLOW[2], COL_YELLOW[3], 0.2)
    end
    text(sx + 8, y + 10, string.upper(game.status), 8, sc[1], sc[2], sc[3])

    -- Play count
    local plays_str = tostring(game.plays) .. " plays"
    local plays_w = #plays_str * 7
    text(x + w - plays_w - 16, y + 42, plays_str, 7, COL_DIM[1], COL_DIM[2], COL_DIM[3])

    -- Size
    text(x + 16, y + 54, game.size, 7, COL_DIM[1], COL_DIM[2], COL_DIM[3])
end

function render()
    local list_w = sw - MARGIN * 2
    vibege.render.clear(COL_BG[1], COL_BG[2], COL_BG[3], 1)
    local y = 0

    -- Title header
    rect(MARGIN, 0, list_w, 44, COL_ACCENT[1], COL_ACCENT[2], COL_ACCENT[3], 1)
    text(MARGIN + 12, 12, "VibeGE Game Store", 14, COL_WHITE[1], COL_WHITE[2], COL_WHITE[3])
    -- Subtitle
    local subtitle = "AI-Friendly Game Overlay"
    text(MARGIN + list_w - #subtitle * 7 - 12, 16, subtitle, 7, COL_WHITE[1], COL_WHITE[2], COL_WHITE[3])
    y = y + 52

    -- Channel tabs
    local tab_w = (list_w / 2) - 6
    draw_tab(MARGIN, y, tab_w, 28, "  Live Games", channel == "live")
    draw_tab(MARGIN + tab_w + 12, y, tab_w, 28, "  Dev Preview", channel == "dev")
    y = y + 36

    -- Instruction bar
    rect(MARGIN, y, list_w, 18, COL_CARD[1], COL_CARD[2], COL_CARD[3], 0.7)
    text(MARGIN + 8, y + 3, "  Arrows: Navigate     Tab: Switch channel     Enter: Launch     Esc: Quit", 7, COL_DIM[1], COL_DIM[2], COL_DIM[3])
    y = y + 26

    -- Game cards
    local active = games_live
    if channel == "dev" then active = games_dev end

    for i, game in ipairs(active) do
        draw_game_card(MARGIN, y, list_w, game, i == selection)
        y = y + CARD_H + GAP
    end

    -- Bottom version bar
    rect(MARGIN, sh - 22, list_w, 18, COL_CARD[1], COL_CARD[2], COL_CARD[3], 0.5)
    local ver = "vibege-runtime v0.1.0"
    text(MARGIN + 8, sh - 20, ver, 7, COL_DIM[1], COL_DIM[2], COL_DIM[3])

    if #active == 0 then
        text(MARGIN + 60, y + 10, "  No games in this channel yet.", 8, COL_DIM[1], COL_DIM[2], COL_DIM[3])
    end
end

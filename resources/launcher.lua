-- VibeGE Game Store Launcher
-- The default experience shown when no game is loaded.
-- Shows Live and Dev game channels, allows selecting and launching games.

local sw, sh = 800, 600
local games_live = {}
local games_dev = {}
local channel = "live" -- "live" or "dev"
local selection = 1
local keys = {}

-- Colors
local bg = {0.05, 0.05, 0.15, 1}
local accent = {0.48, 0.23, 0.93, 1}  -- purple
local accent_light = {0.58, 0.33, 1.0, 1}
local card_bg = {0.10, 0.10, 0.22, 1}
local card_hover = {0.15, 0.15, 0.30, 1}
local text_white = {1, 1, 1, 1}
local text_dim = {0.5, 0.5, 0.6, 1}
local text_green = {0.2, 0.8, 0.4, 1}
local text_yellow = {0.9, 0.7, 0.2, 1}

local function rect(x, y, w, h, r, g, b, a)
    vibege.render.draw_rect(x, y, w, h, r, g, b, a)
end

local function press(key)
    if vibege.input.is_key_pressed(key) then return true end
    return false
end

-- Sample games for demonstration
local function init_games()
    games_live = {
        {name="Pong", desc="Classic paddle arcade", author="VibeGE", status="live", size="1.2 MB", plays=1240},
        {name="Asteroids", desc="Shoot rocks in space", author="VibeGE", status="live", size="2.1 MB", plays=892},
        {name="Snake", desc="Grow the longest snake", author="Community", status="live", size="0.5 MB", plays=456},
    }
    games_dev = {
        {name="Void Drifter", desc="Space exploration survival", author="VibeGE Labs", status="dev", size="3.4 MB", plays=67},
        {name="Block Puzzle", desc="Relaxing puzzle game", author="Community Dev", status="dev", size="1.8 MB", plays=23},
    }
end

function init()
    init_games()
    math.randomseed(os.time())
    print("VibeGE Launcher started")
end

function update(dt)
    -- Channel switching
    if press("tab") or press("left") or press("right") then
        if channel == "live" then channel = "dev" else channel = "live" end
        selection = 1
    end

    -- Navigation
    local active = games_live
    if channel == "dev" then active = games_dev end

    if press("up") and selection > 1 then selection = selection - 1 end
    if press("down") and selection < #active then selection = selection + 1 end

    -- Launch
    if press("enter") or press("space") then
        local game = active[selection]
        if game then
            print("Launching: " .. game.name)
        end
    end

    -- Escape quits
    if press("escape") then
        -- In launcher mode, exit the app
        error("exit", 0)
    end
end

function render()
    vibege.render.clear(bg[1], bg[2], bg[3], bg[4])
    local y_offset = 20
    local margin = 30
    local card_h = 70
    local gap = 8
    local list_x = margin
    local list_w = sw - margin * 2

    -- Title bar
    rect(margin, y_offset, list_w, 50, accent[1], accent[2], accent[3], 1)
    y_offset = y_offset + 60

    -- Channel tabs
    local tab_w = list_w / 2 - 4
    rect(margin, y_offset, tab_w, 30, 
        channel == "live" and accent[1] or card_bg[1],
        channel == "live" and accent[2] or card_bg[2],
        channel == "live" and accent[3] or card_bg[3], 1)
    rect(margin + tab_w + 8, y_offset, tab_w, 30,
        channel == "dev" and accent[1] or card_bg[1],
        channel == "dev" and accent[2] or card_bg[2],
        channel == "dev" and accent[3] or card_bg[3], 1)
    y_offset = y_offset + 40

    -- Instructions
    rect(margin, y_offset, list_w, 20, card_bg[1], card_bg[2], card_bg[3], 0.5)
    y_offset = y_offset + 30

    -- Game list
    local active = games_live
    if channel == "dev" then active = games_dev end

    for i, game in ipairs(active) do
        if i == selection then
            rect(list_x, y_offset, list_w, card_h, accent[1], accent[2], accent[3], 0.3)
        else
            rect(list_x, y_offset, list_w, card_h, card_bg[1], card_bg[2], card_bg[3], 1)
        end

        -- Game name
        local name_y = y_offset + 12
        local name_w = #game.name * 8
        local name_h = 16
        
        -- Status badge
        local status_color = text_green
        if game.status == "dev" then status_color = text_yellow end
        rect(list_x + list_w - 80, y_offset + 10, 65, 18, 
            status_color[1], status_color[2], status_color[3], 0.2)

        -- Play count
        rect(list_x + list_w - 135, y_offset + 10, 50, 18, card_bg[1], card_bg[2], card_bg[3], 0.5)

        y_offset = y_offset + card_h + gap
    end

    -- Bottom instruction bar
    rect(margin, sh - 30, list_w, 22, card_bg[1], card_bg[2], card_bg[3], 0.6)
end

-- VibeGE First Run Wizard
-- Shown when config.toml doesn't exist yet.
-- Guides the player through: hotkey → overlay position → ready.

local step = 1
local total_steps = 3
local hotkey_mod = "ctrl+shift"
local hotkey_key = "v"
local overlay_pos = "center"
local highlight = 0

local COL_BG       = {0.05, 0.05, 0.15}
local COL_ACCENT   = {0.48, 0.23, 0.93}
local COL_WHITE    = {1.0,  1.0,  1.0}
local COL_DIM      = {0.5,  0.5,  0.6}

local function text(x, y, str, size, r, g, b)
    if str and #str > 0 then
        vibege.render.draw_text(x, y, str, size, r, g, b)
    end
end

local function rect(x, y, w, h, r, g, b, a)
    vibege.render.draw_rect(x, y, w, h, r, g, b, a)
end

local function pressed(key)
    return vibege.input.is_key_pressed(key)
end

local function center_text(y, str, size, cr, cg, cb)
    local w = #str * size * 0.5
    text(400 - w, y, str, size, cr, cg, cb)
end

local function draw_button(x, y, w, h, label, is_selected)
    if is_selected then
        rect(x, y, w, h, COL_ACCENT[1], COL_ACCENT[2], COL_ACCENT[3], 1)
    else
        rect(x, y, w, h, COL_BG[1], COL_BG[2], COL_BG[3], 0.5)
    end
    local lw = #label * 6
    text(x + w/2 - lw, y + 6, label, 8, COL_WHITE[1], COL_WHITE[2], COL_WHITE[3])
end

local function draw_radio(x, y, label, is_selected)
    if is_selected then
        rect(x, y, 12, 12, COL_ACCENT[1], COL_ACCENT[2], COL_ACCENT[3], 1)
        rect(x + 3, y + 3, 6, 6, 1, 1, 1, 1)
    else
        rect(x, y, 12, 12, COL_DIM[1], COL_DIM[2], COL_DIM[3], 0.3)
    end
    text(x + 18, y, label, 8, COL_WHITE[1], COL_WHITE[2], COL_WHITE[3])
end

function init()
    highlight = 0
    print("First-run wizard started")
end

function update(dt)
    highlight = highlight + 1
    if step == 1 then
        -- Welcome + choose hotkey
        if pressed("up") then
            if hotkey_mod == "ctrl+shift" then hotkey_mod = "ctrl+alt"
            elseif hotkey_mod == "ctrl+alt" then hotkey_mod = "alt+shift"
            else hotkey_mod = "ctrl+shift" end
        end
        if pressed("down") then
            if hotkey_mod == "ctrl+shift" then hotkey_mod = "alt+shift"
            elseif hotkey_mod == "alt+shift" then hotkey_mod = "ctrl+alt"
            else hotkey_mod = "ctrl+shift" end
        end
        if pressed("left") then
            local keys = {"v", "g", "b", "h", "space", "tab"}
            for i, k in ipairs(keys) do if k == hotkey_key then
                hotkey_key = keys[(i - 2 + #keys) % #keys + 1] break end
            end
        end
        if pressed("right") then
            local keys = {"v", "g", "b", "h", "space", "tab"}
            for i, k in ipairs(keys) do if k == hotkey_key then
                hotkey_key = keys[(i % #keys) + 1] break end
            end
        end
        if pressed("enter") then step = 2 end
    elseif step == 2 then
        -- Choose overlay position
        if pressed("up") then
            if overlay_pos == "center" then overlay_pos = "top-right"
            elseif overlay_pos == "top-right" then overlay_pos = "top-left"
            elseif overlay_pos == "top-left" then overlay_pos = "bottom-right"
            elseif overlay_pos == "bottom-right" then overlay_pos = "bottom-left"
            else overlay_pos = "center" end
        end
        if pressed("down") then
            if overlay_pos == "center" then overlay_pos = "bottom-left"
            elseif overlay_pos == "bottom-left" then overlay_pos = "bottom-right"
            elseif overlay_pos == "bottom-right" then overlay_pos = "top-left"
            elseif overlay_pos == "top-left" then overlay_pos = "top-right"
            else overlay_pos = "center" end
        end
        if pressed("enter") then step = 3 end
    elseif step == 3 then
        -- Ready!
        if pressed("enter") then
            -- Save settings
            vibege.settings.set("hotkey_modifiers", hotkey_mod)
            vibege.settings.set("hotkey_key", hotkey_key)
            vibege.settings.set("position", overlay_pos)
            -- Switch to launcher
            vibege.runtime.switch_game("launcher")
        end
    end

    if pressed("escape") then error("exit", 0) end
end

function render()
    vibege.render.clear(COL_BG[1], COL_BG[2], COL_BG[3], 1)

    -- Top accent bar
    rect(0, 0, 800, 3, COL_ACCENT[1], COL_ACCENT[2], COL_ACCENT[3], 1)

    if step == 1 then
        center_text(60, "Welcome to VibeGE!", 16, COL_WHITE[1], COL_WHITE[2], COL_WHITE[3])
        center_text(90, "The gaming overlay for AI-assisted development", 8, COL_DIM[1], COL_DIM[2], COL_DIM[3])
        rect(150, 130, 500, 1, COL_DIM[1], COL_DIM[2], COL_DIM[3], 0.3)

        center_text(160, "Choose your overlay hotkey", 10, COL_WHITE[1], COL_WHITE[2], COL_WHITE[3])
        center_text(185, "Press Up/Down to change modifier, Left/Right to change key", 7, COL_DIM[1], COL_DIM[2], COL_DIM[3])

        -- Hotkey display
        local hk = hotkey_mod .. " + " .. hotkey_key
        center_text(230, hk, 20, COL_ACCENT[1], COL_ACCENT[2], COL_ACCENT[3])
        center_text(260, "Press Enter to continue", 7, COL_DIM[1], COL_DIM[2], COL_DIM[3])

    elseif step == 2 then
        center_text(60, "Overlay Position", 14, COL_WHITE[1], COL_WHITE[2], COL_WHITE[3])
        center_text(85, "Where should the overlay appear on screen?", 8, COL_DIM[1], COL_DIM[2], COL_DIM[3])

        local opts = {"center", "top-left", "top-right", "bottom-left", "bottom-right"}
        for i, pos in ipairs(opts) do
            draw_radio(250, 130 + (i-1) * 30, pos, overlay_pos == pos)
        end
        center_text(320, "Press Enter to continue", 7, COL_DIM[1], COL_DIM[2], COL_DIM[3])

    elseif step == 3 then
        center_text(80, "You're all set!", 16, COL_WHITE[1], COL_WHITE[2], COL_WHITE[3])
        center_text(110, "Hotkey: " .. hotkey_mod .. " + " .. hotkey_key, 10, COL_ACCENT[1], COL_ACCENT[2], COL_ACCENT[3])
        center_text(135, "Position: " .. overlay_pos, 10, COL_ACCENT[1], COL_ACCENT[2], COL_ACCENT[3])

        center_text(200, "Press any key at any time to open the overlay", 8, COL_DIM[1], COL_DIM[2], COL_DIM[3])
        center_text(220, "and play your installed games while AI works.", 8, COL_DIM[1], COL_DIM[2], COL_DIM[3])

        rect(250, 270, 300, 40, COL_ACCENT[1], COL_ACCENT[2], COL_ACCENT[3], 1)
        center_text(278, "  Get Started", 10, COL_WHITE[1], COL_WHITE[2], COL_WHITE[3])
    end

    -- Step indicator
    for i = 1, total_steps do
        if i == step then
            rect(380 + (i-1) * 20, 360, 12, 12, COL_ACCENT[1], COL_ACCENT[2], COL_ACCENT[3], 1)
        else
            rect(380 + (i-1) * 20, 360, 12, 12, COL_DIM[1], COL_DIM[2], COL_DIM[3], 0.3)
        end
    end
end

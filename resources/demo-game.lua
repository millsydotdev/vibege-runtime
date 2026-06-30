-- Mini demo game — validates input, render, and game switching.
-- A bouncing ball with a paddle you control. Press Escape to go back to launcher.

local ball_x, ball_y = 400, 200
local ball_dx, ball_dy = 3, -2
local paddle_y = 280
local paddle_w = 80
local paddle_h = 10
local score = 0

-- Colors
local BG = {0.05, 0.05, 0.15, 1}
local BALL = {0.3, 0.8, 0.4, 1}
local PADDLE = {0.48, 0.23, 0.93, 1}
local TEXT = {1, 1, 1, 1}

function init()
    print("Demo game started")
end

function update(dt)
    -- Paddle movement
    if vibege.input.is_key_down("left") then paddle_y = paddle_y - 4 end
    if vibege.input.is_key_down("right") then paddle_y = paddle_y + 4 end
    paddle_y = math.max(0, math.min(800 - paddle_w, paddle_y))

    -- Ball movement
    ball_x = ball_x + ball_dx
    ball_y = ball_y + ball_dy

    -- Wall bounce
    if ball_x <= 0 or ball_x >= 790 then ball_dx = -ball_dx end
    if ball_y <= 0 then ball_dy = -ball_dy end

    -- Paddle bounce
    if ball_y >= paddle_y - 8 and ball_y <= paddle_y + paddle_h
       and ball_x >= 100 - 8 and ball_x <= 100 + paddle_w + 8
    then
        ball_dy = -ball_dy
        score = score + 1
        if vibege.audio then vibege.audio.play_bounce() end
    end

    -- Fall off bottom = game over
    if ball_y > 600 then
        score = 0
        ball_x, ball_y = 400, 200
        ball_dx, ball_dy = 3, -2
    end

    -- Escape exits back to Home
    if vibege.input.is_key_down("escape") then
        error("exit", 0)
    end
end

function render()
    vibege.render.clear(BG[1], BG[2], BG[3], BG[4])

    -- Ball
    vibege.render.draw_rect(ball_x, ball_y, 10, 10, BALL[1], BALL[2], BALL[3], 1)

    -- Paddle
    vibege.render.draw_rect(100, paddle_y, paddle_w, paddle_h, PADDLE[1], PADDLE[2], PADDLE[3], 1)

    -- Score
    local score_str = "Score: " .. tostring(score)
    vibege.render.draw_text(350, 10, score_str, 12, TEXT[1], TEXT[2], TEXT[3])

    -- Instructions
    vibege.render.draw_text(10, 560, "Left/Right: Move    Esc: Back to launcher", 7, 0.5, 0.5, 0.6)
end

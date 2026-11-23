-- A fader drawn in Lua: openDJ shows it in the patch's editor window.
-- Click or drag to set the level; the patch multiplies its input by it.
local Fader = pd.Class:new():register("lua-fader")

function Fader:initialize()
    self.inlets, self.outlets = 0, 1
    self.value = 1
    self:set_size(200, 40)
    return true
end

function Fader:paint(g)
    g:set_color(22, 24, 25)
    g:fill_all()
    g:set_color(182, 156, 255)
    g:fill_rounded_rect(4, 4, 192 * self.value, 32, 6)
    g:set_color(255, 255, 255)
    g:draw_text(string.format("%.0f%%", self.value * 100), 8, 12, 100, 14)
end

function Fader:set(x)
    self.value = math.max(0, math.min(1, (x - 4) / 192))
    self:outlet(1, "float", {self.value})
    self:repaint()
end

function Fader:mouse_down(x, y)
    self:set(x)
end

function Fader:mouse_drag(x, y)
    self:set(x)
end


use color::Color;
use graphics::{
    Context,
    AddColor,
    AddRectangle,
    AddImage,
    Draw,
    RelativeTransform2d,
};
use label;
use label::FontSize;
use mouse_state::{
    MouseState,
    Up,
    Down,
};
use opengl_graphics::Gl;
use point::Point;
use rectangle;
use std::default::Default;
use utils::{
    clamp,
    compare_f64s,
};
use ui_context::{
    UIID,
    UIContext,
};
use widget::NumberDialer;

/// Represents the specific elements that the
/// NumberDialer is made up of. This is used to
/// specify which element is Highlighted or Clicked
/// when storing State.
#[deriving(Show, PartialEq, Clone)]
pub enum Element {
    Rect,
    LabelGlyphs,
    /// Represents a value glyph slot at `uint` index
    /// as well as the last mouse.pos.y for comparison
    /// in determining new value.
    ValueGlyph(uint, f64)
}

/// Represents the state of the Button widget.
#[deriving(PartialEq, Clone)]
pub enum State {
    Normal,
    Highlighted(Element),
    Clicked(Element),
}

widget_fns!(NumberDialer, State, NumberDialer(Normal))

/// Create the string to be drawn from the given values
/// and precision. Combine this with the label string if
/// one is given.
fn create_val_string<T: ToString>(val: T, len: uint, precision: u8) -> String {
    let mut val_string = val.to_string();
    // First check we have the correct number of decimal places.
    match (val_string.as_slice().chars().position(|ch| ch == '.'), precision) {
        (None, 0u8) => (),
        (None, _) => {
            val_string.push('.');
            val_string.grow(precision as uint, '0');
        },
        (Some(idx), 0u8) => {
            val_string.truncate(idx);
        },
        (Some(idx), _) => {
            let (len, desired_len) = (val_string.len(), idx + precision as uint + 1u);
            match len.cmp(&desired_len) {
                Greater => val_string.truncate(desired_len),
                Equal => (),
                Less => val_string.grow(desired_len - len, '0'),
            }
        },
    }
    // Now check that the total length matches. We already know that
    // the decimal end of the string is correct, so if the lengths
    // don't match we know we must prepend the difference as '0's.
    match val_string.len().cmp(&len) {
        Less => format!("{}{}", String::from_char(len - val_string.len(), '0'), val_string),
        _ => val_string,
    }
}

/// Return the dimensions of a value glyph slot.
fn value_glyph_slot_width(size: FontSize) -> f64 {
    (size as f64 * 0.75).floor() as f64
}

/// Return the dimensions of the label.
fn label_string_and_dimensions(uic: &mut UIContext,
                               label: Option<(&str, FontSize, Color)>) -> (String, f64, f64) {
    match label {
        None => (String::new(), 0f64, 0f64),
        Some((ref text, size, _)) => {
            let string = format!("{}: ", text);
            let label_width = label::width(uic, size, string.as_slice());
            (string, label_width, size as f64)
        },
    }
}

/// Return the dimensions of value string glyphs.
fn val_string_dimensions(font_size: FontSize,
                         label: Option<(&str, FontSize, Color)>,
                         val_string: &String) -> (f64, f64) {
    let size = match label {
        None => font_size,
        Some((_, label_font_size, _)) => label_font_size,
    };
    let slot_w = value_glyph_slot_width(size);
    let val_string_w = slot_w * val_string.len() as f64;
    (val_string_w, size as f64)
}

/// Determine if the cursor is over the number_dialer and if so, which element.
#[inline]
fn is_over(pos: Point<f64>,
           frame_w: f64,
           mouse_pos: Point<f64>,
           rect_w: f64,
           rect_h: f64,
           l_pos: Point<f64>,
           label_w: f64,
           label_h: f64,
           val_string_w: f64,
           val_string_h: f64,
           val_string_len: uint) -> Option<Element> {
    match rectangle::is_over(pos, mouse_pos, rect_w, rect_h) {
        false => None,
        true => {
            match rectangle::is_over(l_pos, mouse_pos, label_w, label_h) {
                true => Some(LabelGlyphs),
                false => {
                    let frame_w2 = frame_w * 2.0;
                    let slot_rect_pos = Point::new(l_pos.x + label_w, pos.y + frame_w, 0.0);
                    match rectangle::is_over(slot_rect_pos, mouse_pos,
                                             val_string_w, rect_h - frame_w2) {
                        false => Some(Rect),
                        true => {
                            let slot_w = value_glyph_slot_width(val_string_h as u32);
                            let mut slot_pos = slot_rect_pos;
                            for i in range(0u, val_string_len) {
                                if rectangle::is_over(slot_pos, mouse_pos, slot_w, rect_h) {
                                    return Some(ValueGlyph(i, mouse_pos.y))
                                }
                                slot_pos.x += slot_w;
                            }
                            Some(Rect)
                        },
                    }
                },
            }
        },
    }
}

/// Check and return the current state of the NumberDialer.
#[inline]
fn get_new_state(is_over_elem: Option<Element>,
                 prev: State,
                 mouse: MouseState) -> State {
    match (is_over_elem, prev, mouse.left) {
        (Some(_), Normal, Down) => Normal,
        (Some(elem), _, Up) => Highlighted(elem),
        (Some(elem), Highlighted(_), Down) => Clicked(elem),
        (Some(_), Clicked(p_elem), Down) => {
            match p_elem {
                ValueGlyph(idx, _) => Clicked(ValueGlyph(idx, mouse.pos.y)),
                _ => Clicked(p_elem),
            }
        },
        (None, Clicked(p_elem), Down) => {
            match p_elem {
                ValueGlyph(idx, _) => Clicked(ValueGlyph(idx, mouse.pos.y)),
                _ => Clicked(p_elem),
            }
        },
        _ => Normal,
    }
}

/// Return the new value along with it's String representation.
#[inline]
fn get_new_value<T: Num + Copy + Primitive + FromPrimitive + ToPrimitive + ToString>
(val: T, min: T, max: T, idx: uint, y_ord: Ordering, val_string: &String) -> T {
    match y_ord {
        Equal => val,
        _ => {
            let decimal_pos = val_string.as_slice().chars().position(|ch| ch == '.');
            let val_f = val.to_f64().unwrap();
            let min_f = min.to_f64().unwrap();
            let max_f = max.to_f64().unwrap();
            let new_val_f = match decimal_pos {
                None => {
                    let power = val_string.len() - idx - 1u;
                    match y_ord {
                        Less => clamp(val_f + (10f32).powf(power as f32) as f64, min_f, max_f),
                        Greater => clamp(val_f - (10f32).powf(power as f32) as f64, min_f, max_f),
                        _ => val_f,
                    }
                },
                Some(dec_idx) => {
                    let mut power = dec_idx as int - idx as int - 1;
                    if power < -1 { power += 1; }
                    match y_ord {
                        Less => clamp(val_f + (10f32).powf(power as f32) as f64, min_f, max_f),
                        Greater => clamp(val_f - (10f32).powf(power as f32) as f64, min_f, max_f),
                        _ => val_f,
                    }
                },
            };
            FromPrimitive::from_f64(new_val_f).unwrap()
        },
    }
            
}

/*
/// Return a suitable font size for the given pad height.
fn get_font_size(pad_height: f64) -> FontSize {
    clamp(if pad_height % 2.0 == 0.0 { pad_height - 4.0 }
          else { pad_height - 5.0 }, 4.0, 256.0) as FontSize
}
*/

/// Draw the value string glyphs.
#[inline]
fn draw_value_string(win_w: f64,
                     win_h: f64,
                     gl: &mut Gl,
                     uic: &mut UIContext,
                     state: State,
                     slot_y: f64,
                     rect_color: Color,
                     slot_w: f64,
                     pad_h: f64,
                     pos: Point<f64>,
                     size: FontSize,
                     font_color: Color,
                     string: &str) {
    let mut x = 0;
    let y = 0;
    let (font_r, font_g, font_b, font_a) = font_color.as_tuple();
    let context = Context::abs(win_w, win_h).trans(pos.x, pos.y + size as f64);
    let half_slot_w = slot_w / 2.0;
    for (i, ch) in string.chars().enumerate() {
        let character = uic.get_character(size, ch);
        match state {
            Highlighted(elem) => match elem {
                ValueGlyph(idx, _) => {
                    let context_slot_y = slot_y - (pos.y + size as f64);
                    let rect_color = if idx == i { rect_color.highlighted() }
                                     else { rect_color };
                    draw_slot_rect(gl, &context, x as f64, context_slot_y,
                                   size as f64, pad_h, rect_color);
                },
                _ => (),
            },
            Clicked(elem) => match elem {
                ValueGlyph(idx, _) => {
                    let context_slot_y = slot_y - (pos.y + size as f64);
                    let rect_color = if idx == i { rect_color.clicked() }
                                     else { rect_color };
                    draw_slot_rect(gl, &context, x as f64, context_slot_y,
                                   size as f64, pad_h, rect_color);
                },
                _ => (),
            },
            _ => (),
        };
        let x_shift = half_slot_w - (character.glyph.advance().x >> 16) as f64 / 2.0;
        context.trans((x + character.bitmap_glyph.left() + x_shift as i32) as f64,
                      (y - character.bitmap_glyph.top()) as f64)
                        .image(&character.texture)
                        .rgba(font_r, font_g, font_b, font_a)
                        .draw(gl);
        x += slot_w as i32;
    }
}

/// Draw the slot behind the value.
#[inline]
fn draw_slot_rect(gl: &mut Gl, context: &Context,
                  x: f64, y: f64, w: f64, h: f64,
                  color: Color) {
    let (r, g, b, a) = color.as_tuple();
    context.rect(x, y, w, h).rgba(r, g, b, a).draw(gl)
}


/// A context on which the builder pattern can be implemented.
pub struct NumberDialerContext<'a, T> {
    uic: &'a mut UIContext,
    ui_id: UIID,
    value: T,
    min: T,
    max: T,
    pos: Point<f64>,
    width: f64,
    height: f64,
    precision: u8,
    maybe_color: Option<Color>,
    maybe_frame: Option<(f64, Color)>,
    maybe_label: Option<(&'a str, FontSize, Color)>,
    maybe_callback: Option<|T|:'a>,
}

pub trait NumberDialerBuilder
<'a, T: Num + Copy + Primitive + FromPrimitive + ToPrimitive + ToString> {
    /// A number_dialer builder method to be implemented by the UIContext.
    fn number_dialer(&'a mut self, ui_id: UIID, value: T, min: T, max: T,
                     precision: u8) -> NumberDialerContext<'a, T>;
}

impl<'a, T: Num + Copy + Primitive + FromPrimitive + ToPrimitive + ToString>
NumberDialerBuilder<'a, T> for UIContext {
    /// A number_dialer builder method to be implemented by the UIContext.
    fn number_dialer(&'a mut self, ui_id: UIID, value: T, min: T, max: T,
                     precision: u8) -> NumberDialerContext<'a, T> {
        NumberDialerContext {
            uic: self,
            ui_id: ui_id,
            value: clamp(value, min, max),
            min: min,
            max: max,
            pos: Point::new(0.0, 0.0, 0.0),
            width: 128.0,
            height: 48.0,
            precision: precision,
            maybe_color: None,
            maybe_frame: None,
            maybe_label: None,
            maybe_callback: None,
        }
    }
}

impl_callable!(NumberDialerContext, |T|:'a, T)
impl_colorable!(NumberDialerContext, T)
impl_frameable!(NumberDialerContext, T)
impl_labelable!(NumberDialerContext, T)
impl_positionable!(NumberDialerContext, T)
impl_shapeable!(NumberDialerContext, T)

impl<'a, T: Num + Copy + Primitive + FromPrimitive + ToPrimitive + ToString>
::draw::Drawable for NumberDialerContext<'a, T> {
    #[inline]
    /// Draw the number_dialer. When successfully pressed,
    /// or if the value is changed, the given `callback`
    /// function will be called.
    fn draw(&mut self, gl: &mut Gl) {

        let state = *get_state(self.uic, self.ui_id);
        let mouse = self.uic.get_mouse_state();
        let frame_w = match self.maybe_frame { Some((w, _)) => w, None => 0.0 };
        let frame_w2 = frame_w * 2.0;
        let pad_h = self.height - frame_w2;
        //let font_size = get_font_size(pad_h);
        let font_size = 24u32;
        let (label_string, label_w, label_h) =
            label_string_and_dimensions(self.uic, self.maybe_label);
        let val_string_len = self.max.to_string().len() + if self.precision == 0 { 0u }
                                                          else { 1u + self.precision as uint };
        let mut val_string = create_val_string(self.value, val_string_len, self.precision);
        let (val_string_w, val_string_h) =
            val_string_dimensions(font_size, self.maybe_label, &val_string);
        let label_x = self.pos.x + (self.width - (label_w + val_string_w)) / 2.0;
        let label_y = self.pos.y + (self.height - font_size as f64) / 2.0;
        let l_pos = Point::new(label_x, label_y, 0.0);
        let is_over_elem = is_over(self.pos, frame_w, mouse.pos,
                                   self.width, self.height,
                                   l_pos, label_w, label_h,
                                   val_string_w, val_string_h,
                                   val_string.len());
        let new_state = get_new_state(is_over_elem, state, mouse);
        let color = match self.maybe_color { Some(color) => color, None => Default::default() };

        // Draw the widget rectangle.
        rectangle::draw(self.uic.win_w, self.uic.win_h, gl, rectangle::Normal,
                        self.pos, self.width, self.height, self.maybe_frame, color);

        // If there's a label, draw it.
        let (val_string_color, val_string_size) = match self.maybe_label {
            None => (color.plain_contrast(), font_size),
            Some((_, l_size, l_color)) => {
                label::draw(gl, self.uic, l_pos, l_size, l_color, label_string.as_slice());
                (l_color, l_size)
            },
        };

        // Determine new value from the initial state and the new state.
        let new_val = match (state, new_state) {
            (Clicked(elem), Clicked(new_elem)) => {
                match (elem, new_elem) {
                    (ValueGlyph(idx, y), ValueGlyph(_, new_y)) => {
                        get_new_value(self.value, self.min, self.max, idx,
                                      compare_f64s(new_y, y), &val_string)
                    }, _ => self.value,
                }
            }, _ => self.value,
        };

        // If the value has changed, create a new string for val_string.
        if self.value != new_val {
            val_string = create_val_string(new_val, val_string_len, self.precision)
        }

        // Draw the value string.
        let val_string_pos = l_pos + Point::new(label_w, 0.0, 0.0);
        draw_value_string(self.uic.win_w, self.uic.win_h, gl, self.uic, new_state,
                          self.pos.y + frame_w, color,
                          value_glyph_slot_width(val_string_size), pad_h,
                          val_string_pos,
                          val_string_size,
                          val_string_color,
                          val_string.as_slice());

        // Call the `callback` with the new value if the mouse is pressed/released
        // on the widget or if the value has changed.
        if self.value != new_val || match (state, new_state) {
            (Highlighted(_), Clicked(_)) | (Clicked(_), Highlighted(_)) => true,
            _ => false,
        } {
            match self.maybe_callback {
                Some(ref mut callback) => (*callback)(new_val),
                None => ()
            }
        }

        set_state(self.uic, self.ui_id, new_state,
                  self.pos.x, self.pos.y, self.width, self.height);

    }

}


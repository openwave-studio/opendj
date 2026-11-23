/* Runs pd-lua (pdlua/, built as plugdata builds it, with PLUGDATA) in
   openDJ's plugin helper, and hands what its Lua objects draw to the helper,
   which draws it itself. GPL-2.0-or-later. */

#include <stdlib.h>
#include <string.h>
#include "m_pd.h"
#include "m_imp.h"
#include "g_canvas.h"
#include "lua.h"
#include "pdlua/pdlua.h"

/* From pdlua.c (built with PLUGDATA). */
void pdlua_setup(const char *datadir, char *versbuf, int versbuf_length,
    void (*register_class_callback)(const char *));
void pdlua_gfx_repaint(t_pdlua *o, int firsttime);
void pdlua_gfx_mouse_event(t_pdlua *o, int x, int y, int type);

/* One drawing call: its name ("lua_fill_rect" ...), its numbers in order,
   and its text (lua_draw_text) or NULL. */
typedef void (*opendj_draw_fn)(void *obj, int layer, const char *what,
    const double *nums, int n, const char *text);

static opendj_draw_fn draw_fn;

/* The Lua classes that draw: their Pd class names end in ":gfx". */
static t_symbol **gfx_classes;
static int gfx_count;

static void register_class(const char *name)
{
    size_t len = strlen(name);
    if (len < 4 || strcmp(name + len - 4, ":gfx"))
        return;
    gfx_classes = (t_symbol **)resizebytes(gfx_classes,
        gfx_count * sizeof(t_symbol *), (gfx_count + 1) * sizeof(t_symbol *));
    gfx_classes[gfx_count++] = gensym(name);
}

static int draws(t_gobj *y)
{
    t_symbol *s = gensym(class_getname(pd_class(&y->g_pd)));
    for (int i = 0; i < gfx_count; i++)
        if (gfx_classes[i] == s)
            return 1;
    return 0;
}

static void draw(void *obj, int layer, t_symbol *s, int argc, t_atom *argv)
{
    double stack[64], *nums = stack;
    const char *text = NULL;
    int n = 0;
    if (!draw_fn || !obj)
        return;
    if (argc > 64)
        nums = (double *)malloc(argc * sizeof(double));
    if (!nums)
        return;
    for (int i = 0; i < argc; i++)
    {
        if (argv[i].a_type == A_FLOAT)
            nums[n++] = argv[i].a_w.w_float;
        else if (argv[i].a_type == A_SYMBOL)
            text = argv[i].a_w.w_symbol->s_name;
    }
    draw_fn(obj, layer, s->s_name, nums, n, text);
    if (nums != stack)
        free(nums);
}

/* plugdata opens a Lua object's script in its editor; openDJ doesn't. */
void plugdata_forward_message(void *x, t_symbol *s, int argc, t_atom *argv)
{
    (void)x;
    (void)s;
    (void)argc;
    (void)argv;
}

/* Starts pd-lua; it reads `<datadir>/pdlua/pd.lua`. */
void opendj_pdlua_setup(const char *datadir, opendj_draw_fn fn)
{
    char version[MAXPDSTRING];
    draw_fn = fn;
    pdlua_setup(datadir, version, sizeof(version), register_class);
}

/* Calls `found` for each Lua object on `canvas` (not inside subpatches) that
   draws, with where it sits and its size, and routes its drawing to the
   helper. */
void opendj_pdlua_displays(void *canvas, void *ctx,
    void (*found)(void *ctx, void *obj, int x, int y, int w, int h))
{
    t_gobj *y;
    for (y = ((t_canvas *)canvas)->gl_list; y; y = y->g_next)
    {
        if (!draws(y))
            continue;
        t_pdlua *o = (t_pdlua *)y;
        o->gfx.plugdata_draw_callback = draw;
        found(ctx, o, o->pd.te_xpix, o->pd.te_ypix, o->gfx.width, o->gfx.height);
    }
}

void opendj_pdlua_repaint(void *obj)
{
    pdlua_gfx_repaint((t_pdlua *)obj, 0);
}

/* type: 0 down, 1 up, 2 move, 3 drag. */
void opendj_pdlua_mouse(void *obj, int x, int y, int type)
{
    pdlua_gfx_mouse_event((t_pdlua *)obj, x, y, type);
}

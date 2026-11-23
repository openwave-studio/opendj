/* Registers the cyclone and ELSE objects built into openDJ's plugin helper
   with libpd, under their plain names and "cyclone/..." or "else/...", and
   answers [declare -lib cyclone] / [declare -lib else]. GPL-2.0-or-later. */

#include <string.h>
#include "m_pd.h"
#include "m_imp.h"
#include "s_stuff.h"
#include "generated/setups.h"

static t_methodentry *makers(int *n)
{
    t_class *c = pd_objectmaker;
    *n = c->c_nmethod;
#ifdef PDINSTANCE
    return c->c_methods[pd_this->pd_instanceno];
#else
    return c->c_methods;
#endif
}

/* Adds "<lib>/<name>" for every object a library set up after `from`. */
static void prefix_since(int from, const char *lib)
{
    int n, i;
    t_methodentry *m = makers(&n);
    /* class_addcreator may grow the array: copy what's needed first. */
    int count = n - from;
    if (count <= 0)
        return;
    t_methodentry *added = (t_methodentry *)getbytes(count * sizeof(t_methodentry));
    memcpy(added, m + from, count * sizeof(t_methodentry));
    for (i = 0; i < count; i++)
    {
        const char *name = added[i].me_name->s_name;
        char alias[MAXPDSTRING];
        unsigned char *a = added[i].me_arg;
        if (strchr(name, '/'))
            continue;
        snprintf(alias, sizeof(alias), "%s/%s", lib, name);
        if (zgetfn(&pd_objectmaker, gensym(alias)))
            continue;
        class_addcreator((t_newmethod)added[i].me_fun, gensym(alias),
            a[0], a[1], a[2], a[3], a[4], a[5], A_NULL);
    }
    freebytes(added, count * sizeof(t_methodentry));
}

/* The libraries are always loaded: say so when a patch asks for them. */
static int loader(t_canvas *canvas, const char *classname, const char *path)
{
    (void)canvas;
    (void)path;
    return !strcmp(classname, "cyclone") || !strcmp(classname, "else");
}

/* libpd has no MIDI output device; ELSE's MIDI out objects call this. */
#ifndef _MSC_VER
__attribute__((weak))
#endif
void sys_putmidibyte(int portno, int byte)
{
    (void)portno;
    (void)byte;
}

void opendj_pd_libs_setup(void)
{
    int n;
    makers(&n);
    CYCLONE_SETUPS
    prefix_since(n, "cyclone");
    makers(&n);
    ELSE_SETUPS
    prefix_since(n, "else");
    sys_register_loader(loader);
}

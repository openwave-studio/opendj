/* Pd 0.56 added theme colours that some cyclone and ELSE objects read when
   they draw themselves in Pd's editor. libpd (Pd 0.55) draws nothing, so
   fixed colours stand in for those fields. Included before every file. */
#ifndef OPENDJ_PD_GUI_COMPAT_H
#define OPENDJ_PD_GUI_COMPAT_H
#include "m_pd.h"
#include "g_canvas.h"
struct opendj_gui_colors {
    int i_foregroundcolor, i_backgroundcolor, i_selectcolor, i_gopcolor;
};
static struct opendj_gui_colors opendj_gui_colors = {0x000000, 0xffffff, 0x0000ff, 0xff0000};
#undef THISGUI
#define THISGUI (&opendj_gui_colors)
#endif

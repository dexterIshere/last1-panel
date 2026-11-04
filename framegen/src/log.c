#include "log.h"

#include "config.h"

#include <fcntl.h>
#include <pthread.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

/* Last1 Panel : LAST1_FG_LOG_FILE (posé par last1-panel-cli run seulement, jamais hérité) fait
 * écrire le journal dans ce fichier, pour les rapports des parties d'essai ; sinon stderr.
 * Ouvert en ajout, sans suivre de lien symbolique, lisible par le seul joueur. */
static FILE *g_out;
static pthread_once_t g_out_once = PTHREAD_ONCE_INIT;

static void open_out(void)
{
    g_out = stderr;
    const char *path = getenv("LAST1_FG_LOG_FILE");
    if (path == NULL || *path != '/')
        return;
    int fd = open(path, O_WRONLY | O_APPEND | O_CREAT | O_NOFOLLOW | O_CLOEXEC, 0600);
    if (fd < 0)
        return;
    FILE *file = fdopen(fd, "a");
    if (file == NULL) {
        (void)close(fd);
        return;
    }
    setvbuf(file, NULL, _IOLBF, 0);
    g_out = file;
}

void afmf_log(enum afmf_log_level level, const char *fmt, ...)
{
    static const char *const names[] = {"error", "warn", "info", "debug"};

    if ((int)level > afmf_config_get()->log_level)
        return;
    (void)pthread_once(&g_out_once, open_out);

    va_list ap;
    va_start(ap, fmt);
    flockfile(g_out);
    (void)fprintf(g_out, "[last1-framegen %s] ", names[level]);
    (void)vfprintf(g_out, fmt, ap);
    (void)fputc('\n', g_out);
    funlockfile(g_out);
    va_end(ap);
}

/*
 * xezim_bridge.c — VPI control bridge for mixed e + SystemVerilog
 * testcases.
 *
 * The simulator (xezim) is the master: after elaboration this module
 * enters a control loop that reads one command per line from stdin and
 * answers exactly one response line per command on stdout:
 *
 *   SET <path> <value>   -> OK            (vpi_put_value, vpiNoDelay)
 *   GET <path>           -> VAL <value>   (vpi_get_value, vpiIntVal)
 *   RUN <delay>          -> OK, then READY once the callback re-enters
 *                           the loop after advancing simulation time
 *                           (cbAfterDelay)
 *   FINISH               -> BYE           (vpi_control(vpiFinish))
 *
 * Anything unknown answers `ERR <reason>`. Lines the simulator itself
 * prints are simply interleaved with these; the e1647-rs side filters
 * for the response prefixes (READY / OK / VAL / ERR / BYE).
 *
 * Built by the Dockerfile:
 *   cc -shared -fPIC -I<xezim>/include -o bridge.so xezim_bridge.c
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "vpi_user.h"

static void control_loop(void);

/* After a RUN the simulation advanced; re-enter the control loop and
 * tell the driver we can accept commands again. */
static PLI_INT32 reenter_cb(p_cb_data cb_data)
{
    (void)cb_data;
    printf("READY\n");
    fflush(stdout);
    control_loop();
    return 0;
}

static void control_loop(void)
{
    char line[1024];

    while (fgets(line, sizeof line, stdin)) {
        line[strcspn(line, "\r\n")] = '\0';

        if (strncmp(line, "SET ", 4) == 0) {
            char path[512];
            long value;
            vpiHandle handle;
            s_vpi_value value_data;
            if (sscanf(line + 4, "%511s %ld", path, &value) != 2) {
                printf("ERR usage: SET <path> <value>\n");
                fflush(stdout);
                continue;
            }
            handle = vpi_handle_by_name(path, NULL);
            if (handle == NULL) {
                printf("ERR no such signal '%s'\n", path);
                fflush(stdout);
                continue;
            }
            value_data.format = vpiIntVal;
            value_data.value.integer = (PLI_INT32)value;
            vpi_put_value(handle, &value_data, NULL, vpiNoDelay);
            vpi_release_handle(handle);
            printf("OK\n");
            fflush(stdout);
        } else if (strncmp(line, "GET ", 4) == 0) {
            char path[512];
            vpiHandle handle;
            s_vpi_value value_data;
            if (sscanf(line + 4, "%511s", path) != 1) {
                printf("ERR usage: GET <path>\n");
                fflush(stdout);
                continue;
            }
            handle = vpi_handle_by_name(path, NULL);
            if (handle == NULL) {
                printf("ERR no such signal '%s'\n", path);
                fflush(stdout);
                continue;
            }
            value_data.format = vpiIntVal;
            vpi_get_value(handle, &value_data);
            vpi_release_handle(handle);
            printf("VAL %d\n", value_data.value.integer);
            fflush(stdout);
        } else if (strncmp(line, "RUN ", 4) == 0) {
            s_cb_data callback_data;
            s_vpi_time delay_time;
            long delay = atol(line + 4);
            delay_time.type = vpiSimTime;
            delay_time.high = 0;
            delay_time.low = (PLI_UINT32)delay;
            memset(&callback_data, 0, sizeof callback_data);
            callback_data.reason = cbAfterDelay;
            callback_data.cb_rtn = reenter_cb;
            callback_data.time = &delay_time;
            callback_data.index = 0;
            callback_data.user_data = NULL;
            if (vpi_register_cb(&callback_data) == NULL) {
                printf("ERR could not register the time callback\n");
                fflush(stdout);
                continue;
            }
            printf("OK\n");
            fflush(stdout);
            return; /* the callback re-enters the loop */
        } else if (strcmp(line, "FINISH") == 0) {
            printf("BYE\n");
            fflush(stdout);
            vpi_control(vpiFinish);
            return;
        } else if (line[0] == '\0') {
            /* ignore empty input lines */
        } else {
            printf("ERR unknown command\n");
            fflush(stdout);
        }
    }
    /* stdin closed without FINISH: end the simulation. */
    vpi_control(vpiFinish);
}

static PLI_INT32 end_of_compile_cb(p_cb_data cb_data)
{
    (void)cb_data;
    printf("READY\n");
    fflush(stdout);
    control_loop();
    return 0;
}

static void register_bridge(void)
{
    s_cb_data callback_data;
    memset(&callback_data, 0, sizeof callback_data);
    callback_data.reason = cbEndOfCompile;
    callback_data.cb_rtn = end_of_compile_cb;
    callback_data.index = 0;
    callback_data.user_data = NULL;
    vpi_register_cb(&callback_data);
}

void (*vlog_startup_routines[])(void) = {
    register_bridge,
    0
};

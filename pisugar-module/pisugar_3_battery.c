/*
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 2 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <http://www.gnu.org/licenses/>.
 */

/* Based heavily on
 * https://git.kernel.org/cgit/linux/kernel/git/stable/linux-stable.git/tree/drivers/power/test_power.c?id=refs/tags/v4.2.6
 */
/* 
 * Fork from https://github.com/hoelzro/linux-fake-battery-module
 * https://docs.kernel.org/power/power_supply_class.html
 */

#include <linux/fs.h>
#include <linux/i2c.h>
#include <linux/init.h>
#include <linux/kernel.h>
#include <linux/kthread.h>
#include <linux/miscdevice.h>
#include <linux/module.h>
#include <linux/power_supply.h>
#include <linux/sched.h>

#define PISUGAR_3_BAT_I2C_BUS  0x01
#define PISUGAR_3_BAT_I2C_ADDR 0x57

#define TOTAL_LIFE_SECONDS          (3*60*60)
#define TOTAL_CHARGE                (2000*1000)     // uAH
#define TOTAL_CHARGE_FULL_SECONDS   (60*60)

enum pisugar_3_bat_reg {
    PISUGAR_3_VER = 0x00,
    PISUGAR_3_MOD = 0x01,
    PISUGAR_3_CTL1 = 0x02,
    PISUGAR_3_TEMP = 0x04,
    PISUGAR_3_CAP = 0x2A,
    PISUGAR_3_VOL_H = 0x22,
    PISGUAR_3_VOL_L = 0x23,
};

#define PISUGAR_3_VER_3   3
#define PISUGAR_3_MOD_APP 0x0F

#define PISUGAR_3_MSK_CTR1_USB   (1 << 7)
#define PISUGAR_3_MSK_CTR1_CH_EN (1 << 6)

static short int i2c_bus = PISUGAR_3_BAT_I2C_BUS;
static short int i2c_addr = PISUGAR_3_BAT_I2C_ADDR;

module_param(i2c_bus, short, S_IRUSR | S_IWUSR | S_IRGRP | S_IWGRP);
MODULE_PARM_DESC(i2c_bus, "I2C bus default 0x01");

module_param(i2c_addr, short, S_IRUSR | S_IWUSR | S_IRGRP | S_IWGRP);
MODULE_PARM_DESC(i2c_addr, "I2C addr default 0x57");

static int pisugar_3_battery_get_property1(struct power_supply *psy,
                                           enum power_supply_property psp,
                                           union power_supply_propval *val);

static int pisugar_3_ac_get_property(struct power_supply *psy,
                                     enum power_supply_property psp,
                                     union power_supply_propval *val);

static struct task_struct *pisugar_3_monitor_task = NULL;

static struct battery_status {
    int status;
    int capacity_level;
    int capacity;
    int time_left;
    int voltage;
    int temperature;
} pisugar_3_battery_statuses[1] = {{
    .status = POWER_SUPPLY_STATUS_FULL,
    .capacity_level = POWER_SUPPLY_CAPACITY_LEVEL_FULL,
    .capacity = 100,
    .time_left = TOTAL_LIFE_SECONDS,
    .voltage = 4200 * 1000, // uV
    .temperature = 30
}};

static int ac_status = 1;

static char *pisugar_3_ac_supplies[] = {
    "BAT0",
};

static enum power_supply_property pisugar_3_battery_properties[] = {
    POWER_SUPPLY_PROP_STATUS,
    POWER_SUPPLY_PROP_CHARGE_TYPE,
    POWER_SUPPLY_PROP_HEALTH,
    POWER_SUPPLY_PROP_PRESENT,
    POWER_SUPPLY_PROP_TECHNOLOGY,
    POWER_SUPPLY_PROP_CHARGE_EMPTY,
    POWER_SUPPLY_PROP_CHARGE_FULL_DESIGN,
    POWER_SUPPLY_PROP_CHARGE_FULL,
    POWER_SUPPLY_PROP_CHARGE_NOW,
    POWER_SUPPLY_PROP_CAPACITY,
    POWER_SUPPLY_PROP_CAPACITY_LEVEL,
    POWER_SUPPLY_PROP_TIME_TO_EMPTY_AVG,
    POWER_SUPPLY_PROP_TIME_TO_FULL_NOW,
    POWER_SUPPLY_PROP_MODEL_NAME,
    POWER_SUPPLY_PROP_MANUFACTURER,
    POWER_SUPPLY_PROP_SERIAL_NUMBER,
    POWER_SUPPLY_PROP_TEMP,
    POWER_SUPPLY_PROP_VOLTAGE_NOW,
};

static enum power_supply_property pisugar_3_ac_properties[] = {
    POWER_SUPPLY_PROP_ONLINE,
};

static struct power_supply_desc descriptions[] = {
    {
        .name = "BAT0",
        .type = POWER_SUPPLY_TYPE_BATTERY,
        .properties = pisugar_3_battery_properties,
        .num_properties = ARRAY_SIZE(pisugar_3_battery_properties),
        .get_property = pisugar_3_battery_get_property1,
    },

    {
        .name = "AC0",
        .type = POWER_SUPPLY_TYPE_MAINS,
        .properties = pisugar_3_ac_properties,
        .num_properties = ARRAY_SIZE(pisugar_3_ac_properties),
        .get_property = pisugar_3_ac_get_property,
    },
};

static struct power_supply_config configs[] = {
    {},
    {},
    {
        .supplied_to = pisugar_3_ac_supplies,
        .num_supplicants = ARRAY_SIZE(pisugar_3_ac_supplies),
    },
};

static struct power_supply *supplies[sizeof(descriptions) / sizeof(descriptions[0])];

#define prefixed(s, prefix) (!strncmp((s), (prefix), sizeof(prefix) - 1))

static int pisugar_3_battery_generic_get_property(struct power_supply *psy,
                                                  enum power_supply_property psp,
                                                  union power_supply_propval *val,
                                                  struct battery_status *status)
{
    switch (psp) {
        case POWER_SUPPLY_PROP_MANUFACTURER:
            val->strval = "PiSugar";
            break;
        case POWER_SUPPLY_PROP_STATUS:
            val->intval = status->status;
            break;
        case POWER_SUPPLY_PROP_CHARGE_TYPE:
            val->intval = POWER_SUPPLY_CHARGE_TYPE_STANDARD;
            break;
        case POWER_SUPPLY_PROP_HEALTH:
            val->intval = POWER_SUPPLY_HEALTH_GOOD;
            break;
        case POWER_SUPPLY_PROP_PRESENT:
            val->intval = 1;
            break;
        case POWER_SUPPLY_PROP_TECHNOLOGY:
            val->intval = POWER_SUPPLY_TECHNOLOGY_LION;
            break;
        case POWER_SUPPLY_PROP_CAPACITY:
            val->intval = status->capacity;
            break;
        case POWER_SUPPLY_PROP_CAPACITY_LEVEL:
            val->intval = status->capacity_level;
            break;
        case POWER_SUPPLY_PROP_CHARGE_EMPTY:
            val->intval = 0;
            break;
        case POWER_SUPPLY_PROP_CHARGE_NOW:
            val->intval = status->capacity * TOTAL_CHARGE / 100;
            break;
        case POWER_SUPPLY_PROP_CHARGE_FULL_DESIGN:
        case POWER_SUPPLY_PROP_CHARGE_FULL:
            val->intval = TOTAL_CHARGE;
            break;
        case POWER_SUPPLY_PROP_TIME_TO_EMPTY_AVG:
            val->intval = status->time_left;
            break;
        case POWER_SUPPLY_PROP_TIME_TO_FULL_NOW:
            val->intval = (100 - status->capacity) * TOTAL_CHARGE_FULL_SECONDS / 100;
            break;
        case POWER_SUPPLY_PROP_TEMP:
            val->intval = status->temperature;
            break;
        case POWER_SUPPLY_PROP_VOLTAGE_NOW:
            val->intval = status->voltage;
            break;
        default:
            pr_info("%s: some properties deliberately report errors.\n", __func__);
            return -EINVAL;
    }
    return 0;
};

static int pisugar_3_battery_get_property1(struct power_supply *psy,
                                           enum power_supply_property psp,
                                           union power_supply_propval *val)
{
    switch (psp) {
        case POWER_SUPPLY_PROP_MODEL_NAME:
            val->strval = "PiSugar battery 0";
            break;
        case POWER_SUPPLY_PROP_SERIAL_NUMBER:
            val->strval = "";
            break;
        default:
            return pisugar_3_battery_generic_get_property(psy, psp, val, &pisugar_3_battery_statuses[0]);
    }
    return 0;
}

static int pisugar_3_ac_get_property(struct power_supply *psy,
                                     enum power_supply_property psp,
                                     union power_supply_propval *val)
{
    switch (psp) {
        case POWER_SUPPLY_PROP_ONLINE:
            val->intval = ac_status;
            break;
        default:
            return -EINVAL;
    }
    return 0;
}

#define CHECK_VALID(val) ((val) >= 0 && (val) <= 255)

static int pisugar_3_monitor(void *args)
{
    struct i2c_client *pisugar_3_client = NULL;
    struct i2c_adapter *adapter = NULL;
    struct i2c_board_info board_info = {I2C_BOARD_INFO("pisugar_3_battery", i2c_addr)};

    // create i2c pisugar_3_client
    adapter = i2c_get_adapter(i2c_bus);
    if (adapter == NULL) {
        printk(KERN_ERR "Unable to get i2c adapter!");
        return -1;
    }
    pisugar_3_client = i2c_new_client_device(adapter, &board_info);
    if (pisugar_3_client == NULL) {
        printk(KERN_ERR "Unable to create i2c client!");
        return -1;
    }

    while (true) {
        set_current_state(TASK_UNINTERRUPTIBLE);
        if (kthread_should_stop()) break;

        int ver = i2c_smbus_read_byte_data(pisugar_3_client, PISUGAR_3_VER);
        int mode = i2c_smbus_read_byte_data(pisugar_3_client, PISUGAR_3_MOD);
        if (ver != PISUGAR_3_VER_3 || mode != PISUGAR_3_MOD_APP) {
            ac_status = 0;  // device offlie
        } else {
            int ctl1 = i2c_smbus_read_byte_data(pisugar_3_client, PISUGAR_3_CTL1);
            if (!CHECK_VALID(ctl1)) {
                goto sleep;
            }
            bool ch_en = ctl1 & PISUGAR_3_MSK_CTR1_CH_EN;
            bool online = ctl1 & PISUGAR_3_MSK_CTR1_USB ? 1 : 0;
            ac_status = online ? 1 : 0;

            // temperature, zero point -40c
            int temperature = i2c_smbus_read_byte_data(pisugar_3_client, PISUGAR_3_TEMP);
            if (!CHECK_VALID(temperature)) {
                goto sleep;
            }
            pisugar_3_battery_statuses->temperature = temperature - 40;

            // battery capacity
            int cap = i2c_smbus_read_byte_data(pisugar_3_client, PISUGAR_3_CAP);
            cap = cap > 100 ? 100 : cap;
            pisugar_3_battery_statuses->capacity = cap;
            if (cap > 95) {
                pisugar_3_battery_statuses->capacity_level = POWER_SUPPLY_CAPACITY_LEVEL_FULL;
            } else if (cap > 85) {
                pisugar_3_battery_statuses->capacity_level = POWER_SUPPLY_CAPACITY_LEVEL_HIGH;
            } else if (cap > 40) {
                pisugar_3_battery_statuses->capacity_level = POWER_SUPPLY_CAPACITY_LEVEL_NORMAL;
            } else if (cap > 30) {
                pisugar_3_battery_statuses->capacity_level = POWER_SUPPLY_CAPACITY_LEVEL_LOW;
            } else {
                pisugar_3_battery_statuses->capacity_level = POWER_SUPPLY_CAPACITY_LEVEL_CRITICAL;
            }

            // time left
            pisugar_3_battery_statuses->time_left = cap * TOTAL_LIFE_SECONDS / 100;

            // voltage
            int vol_h = i2c_smbus_read_byte_data(pisugar_3_client, PISUGAR_3_VOL_H);
            int vol_l = i2c_smbus_read_byte_data(pisugar_3_client, PISGUAR_3_VOL_L);
            if (!CHECK_VALID(vol_h) || !CHECK_VALID(vol_l)) {
                goto sleep;
            }
            pisugar_3_battery_statuses->voltage = (vol_h << 8) | vol_l;

            // charging status
            if (online && ch_en) {
                if (cap > 95) {
                    pisugar_3_battery_statuses->status = POWER_SUPPLY_STATUS_FULL;
                } else {
                    pisugar_3_battery_statuses->status = POWER_SUPPLY_STATUS_CHARGING;
                }
            } else {
                pisugar_3_battery_statuses->status = POWER_SUPPLY_STATUS_DISCHARGING;
            }
        }
    sleep:
        set_current_state(TASK_RUNNING);
        schedule_timeout(HZ);
    }

    i2c_unregister_device(pisugar_3_client);
    pisugar_3_client = NULL;

    return 0;
}

static int __init pisugar_3_battery_init(void)
{
    int result;
    int i;

    // create a monitor kthread
    pisugar_3_monitor_task = kthread_run(pisugar_3_monitor, NULL, "pisugar_3_monitor");
    if (pisugar_3_monitor_task == NULL) {
        goto error;
    }

    // register power supply
    for (i = 0; i < ARRAY_SIZE(descriptions); i++) {
        supplies[i] = power_supply_register(NULL, &descriptions[i], &configs[i]);
        if (IS_ERR(supplies[i])) {
            printk(KERN_ERR "Unable to register power supply %d in pisugar_3_battery\n", i);
            goto error;
        }
    }

    printk(KERN_INFO "loaded pisugar_3_battery module\n");
    return 0;

error:
    if (pisugar_3_monitor_task) {
        kthread_stop(pisugar_3_monitor_task);
        pisugar_3_monitor_task = NULL;
    }
    while (--i >= 0) {
        power_supply_unregister(supplies[i]);
    }
    return -1;
}

static void __exit pisugar_3_battery_exit(void)
{
    int i;

    if (pisugar_3_monitor_task) {
        kthread_stop(pisugar_3_monitor_task);
        pisugar_3_monitor_task = NULL;
    }

    for (i = ARRAY_SIZE(descriptions) - 1; i >= 0; i--) {
        power_supply_unregister(supplies[i]);
    }

    printk(KERN_INFO "unloaded pisugar_3_battery module\n");
}

module_init(pisugar_3_battery_init);
module_exit(pisugar_3_battery_exit);

MODULE_AUTHOR("The PiSugar Team <pisugar.zero@gmail.com>");
MODULE_DESCRIPTION("PiSugar 3 battery driver");
MODULE_LICENSE("GPL");

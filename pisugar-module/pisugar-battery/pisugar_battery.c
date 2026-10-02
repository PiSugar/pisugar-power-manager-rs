// SPDX-License-Identifier: GPL-2.0-or-later
/* PiSugar 2/3 auto-detecting Linux power_supply driver. */

#include <linux/err.h>
#include <linux/i2c.h>
#include <linux/init.h>
#include <linux/kernel.h>
#include <linux/module.h>
#include <linux/mutex.h>
#include <linux/power_supply.h>
#include <linux/workqueue.h>

#define PISUGAR_I2C_BUS_DEFAULT 1
#define PISUGAR2_ADDR 0x75
#define PISUGAR3_ADDR_DEFAULT 0x57
#define POLL_INTERVAL (2 * HZ)
#define TOTAL_CHARGE_UAH 2000000
#define TOTAL_LIFE_SECONDS (3 * 60 * 60)
#define TOTAL_CHARGE_SECONDS (60 * 60)

enum pisugar_model {
	PISUGAR_NONE,
	PISUGAR2_IP5209,
	PISUGAR2_IP5312,
	PISUGAR3,
};

struct voltage_point {
	int mv;
	int percent;
};

static const struct voltage_point ip5209_curve[] = {
	{4160, 100}, {4050, 95}, {4000, 80}, {3920, 65}, {3860, 40},
	{3790, 25}, {3660, 10}, {3520, 6}, {3490, 3}, {3100, 0},
};

static const struct voltage_point ip5312_curve[] = {
	{4100, 100}, {4050, 95}, {3900, 88}, {3800, 77}, {3700, 65},
	{3620, 55}, {3580, 49}, {3490, 25}, {3320, 4}, {3100, 0},
};

static int i2c_bus = PISUGAR_I2C_BUS_DEFAULT;
static int pisugar3_addr = PISUGAR3_ADDR_DEFAULT;
/* -1: auto, 0: IP5209 (standard), 1: IP5312 (Pro). */
static int pisugar2_model = -1;
module_param(i2c_bus, int, 0444);
MODULE_PARM_DESC(i2c_bus, "I2C bus number (default 1)");
module_param(pisugar3_addr, int, 0444);
MODULE_PARM_DESC(pisugar3_addr, "PiSugar 3 I2C address (default 0x57)");
module_param(pisugar2_model, int, 0444);
MODULE_PARM_DESC(pisugar2_model, "PiSugar 2 chip override: -1 auto, 0 IP5209, 1 IP5312");

struct pisugar_data {
	struct i2c_adapter *adapter;
	struct i2c_client *client;
	struct power_supply *battery;
	struct power_supply *ac;
	struct delayed_work monitor_work;
	struct mutex lock;
	enum pisugar_model model;
	int capacity;
	int capacity_level;
	int status;
	int voltage_uv;
	int temperature;
	int online;
};

static struct pisugar_data pisugar;

static int read_reg(struct i2c_client *client, u8 reg)
{
	return i2c_smbus_read_byte_data(client, reg);
}

static int ip5209_voltage(struct i2c_client *client)
{
	int low = read_reg(client, 0xa2);
	int high = read_reg(client, 0xa3);
	int raw;

	if (low < 0 || high < 0)
		return -EIO;
	if (high & 0x20) {
		raw = (s16)(((high | 0xc0) << 8) | low);
		return 2600 - (raw * 26855) / 100000;
	}
	raw = ((high & 0x1f) << 8) | low;
	return 2600 + (raw * 26855) / 100000;
}

static int ip5312_voltage(struct i2c_client *client)
{
	int low = read_reg(client, 0xd0);
	int high = read_reg(client, 0xd1);
	int raw;

	if (low < 0 || high < 0 || (low == 0 && high == 0))
		return -EIO;
	raw = ((high & 0x3f) << 8) | low;
	return 2600 + (raw * 26855) / 100000;
}

static bool voltage_is_plausible(int mv)
{
	return mv >= 2800 && mv <= 4600;
}

static int voltage_to_capacity(int mv, const struct voltage_point *curve,
			       size_t count)
{
	size_t i;

	if (mv >= curve[0].mv)
		return 100;
	if (mv <= curve[count - 1].mv)
		return 0;

	for (i = 1; i < count; i++) {
		if (mv >= curve[i].mv)
			return curve[i].percent +
			       (mv - curve[i].mv) *
			       (curve[i - 1].percent - curve[i].percent) /
			       (curve[i - 1].mv - curve[i].mv);
	}
	return 0;
}

static void update_capacity_level(struct pisugar_data *data)
{
	if (data->capacity > 95)
		data->capacity_level = POWER_SUPPLY_CAPACITY_LEVEL_FULL;
	else if (data->capacity > 85)
		data->capacity_level = POWER_SUPPLY_CAPACITY_LEVEL_HIGH;
	else if (data->capacity > 40)
		data->capacity_level = POWER_SUPPLY_CAPACITY_LEVEL_NORMAL;
	else if (data->capacity > 15)
		data->capacity_level = POWER_SUPPLY_CAPACITY_LEVEL_LOW;
	else
		data->capacity_level = POWER_SUPPLY_CAPACITY_LEVEL_CRITICAL;
}

static int update_pisugar3(struct pisugar_data *data)
{
	int ctl = read_reg(data->client, 0x02);
	int temp = read_reg(data->client, 0x04);
	int cap = read_reg(data->client, 0x2a);
	int high = read_reg(data->client, 0x22);
	int low = read_reg(data->client, 0x23);

	if (ctl < 0 || temp < 0 || cap < 0 || high < 0 || low < 0)
		return -EIO;
	data->online = !!(ctl & BIT(7));
	data->capacity = clamp(cap, 0, 100);
	data->temperature = (temp - 40) * 10; /* tenths of a degree C */
	data->voltage_uv = ((high << 8) | low) * 1000;
	if (data->online && (ctl & BIT(6)))
		data->status = data->capacity > 95 ? POWER_SUPPLY_STATUS_FULL :
			POWER_SUPPLY_STATUS_CHARGING;
	else
		data->status = POWER_SUPPLY_STATUS_DISCHARGING;
	update_capacity_level(data);
	return 0;
}

static int update_pisugar2(struct pisugar_data *data)
{
	const struct voltage_point *curve;
	size_t curve_size;
	int charging;
	int mv;

	if (data->model == PISUGAR2_IP5312) {
		mv = ip5312_voltage(data->client);
		charging = read_reg(data->client, 0xdd);
		data->online = charging == 0x1f;
		curve = ip5312_curve;
		curve_size = ARRAY_SIZE(ip5312_curve);
	} else {
		mv = ip5209_voltage(data->client);
		charging = read_reg(data->client, 0x55);
		data->online = charging >= 0 && !!(charging & BIT(4));
		curve = ip5209_curve;
		curve_size = ARRAY_SIZE(ip5209_curve);
	}
	if (!voltage_is_plausible(mv) || charging < 0)
		return -EIO;

	data->voltage_uv = mv * 1000;
	data->capacity = voltage_to_capacity(mv, curve, curve_size);
	data->temperature = 300;
	data->status = data->online ?
		(data->capacity > 95 ? POWER_SUPPLY_STATUS_FULL :
		 POWER_SUPPLY_STATUS_CHARGING) : POWER_SUPPLY_STATUS_DISCHARGING;
	update_capacity_level(data);
	return 0;
}

static void pisugar_monitor(struct work_struct *work)
{
	struct pisugar_data *data = container_of(to_delayed_work(work),
						 struct pisugar_data, monitor_work);
	int old_capacity, old_status, old_voltage, old_temperature, old_online;
	bool changed;
	int ret;

	mutex_lock(&data->lock);
	old_capacity = data->capacity;
	old_status = data->status;
	old_voltage = data->voltage_uv;
	old_temperature = data->temperature;
	old_online = data->online;
	ret = data->model == PISUGAR3 ? update_pisugar3(data) :
		update_pisugar2(data);
	changed = old_capacity != data->capacity || old_status != data->status ||
		old_voltage != data->voltage_uv ||
		old_temperature != data->temperature || old_online != data->online;
	mutex_unlock(&data->lock);
	if (!ret && changed) {
		power_supply_changed(data->battery);
		power_supply_changed(data->ac);
	}
	schedule_delayed_work(&data->monitor_work, POLL_INTERVAL);
}

static enum power_supply_property battery_properties[] = {
	POWER_SUPPLY_PROP_STATUS, POWER_SUPPLY_PROP_HEALTH,
	POWER_SUPPLY_PROP_PRESENT, POWER_SUPPLY_PROP_TECHNOLOGY,
	POWER_SUPPLY_PROP_CHARGE_FULL_DESIGN, POWER_SUPPLY_PROP_CHARGE_FULL,
	POWER_SUPPLY_PROP_CHARGE_NOW, POWER_SUPPLY_PROP_CAPACITY,
	POWER_SUPPLY_PROP_CAPACITY_LEVEL, POWER_SUPPLY_PROP_TIME_TO_EMPTY_AVG,
	POWER_SUPPLY_PROP_TIME_TO_FULL_NOW, POWER_SUPPLY_PROP_MODEL_NAME,
	POWER_SUPPLY_PROP_MANUFACTURER, POWER_SUPPLY_PROP_TEMP,
	POWER_SUPPLY_PROP_VOLTAGE_NOW,
};

static int battery_get_property(struct power_supply *psy,
				enum power_supply_property prop,
				union power_supply_propval *val)
{
	struct pisugar_data *data = power_supply_get_drvdata(psy);

	mutex_lock(&data->lock);
	switch (prop) {
	case POWER_SUPPLY_PROP_STATUS: val->intval = data->status; break;
	case POWER_SUPPLY_PROP_HEALTH: val->intval = POWER_SUPPLY_HEALTH_GOOD; break;
	case POWER_SUPPLY_PROP_PRESENT: val->intval = 1; break;
	case POWER_SUPPLY_PROP_TECHNOLOGY: val->intval = POWER_SUPPLY_TECHNOLOGY_LION; break;
	case POWER_SUPPLY_PROP_CHARGE_FULL_DESIGN:
	case POWER_SUPPLY_PROP_CHARGE_FULL: val->intval = TOTAL_CHARGE_UAH; break;
	case POWER_SUPPLY_PROP_CHARGE_NOW:
		val->intval = data->capacity * TOTAL_CHARGE_UAH / 100; break;
	case POWER_SUPPLY_PROP_CAPACITY: val->intval = data->capacity; break;
	case POWER_SUPPLY_PROP_CAPACITY_LEVEL: val->intval = data->capacity_level; break;
	case POWER_SUPPLY_PROP_TIME_TO_EMPTY_AVG:
		val->intval = data->capacity * TOTAL_LIFE_SECONDS / 100; break;
	case POWER_SUPPLY_PROP_TIME_TO_FULL_NOW:
		val->intval = (100 - data->capacity) * TOTAL_CHARGE_SECONDS / 100; break;
	case POWER_SUPPLY_PROP_MODEL_NAME:
		val->strval = data->model == PISUGAR3 ? "PiSugar 3" :
			(data->model == PISUGAR2_IP5312 ? "PiSugar 2 Pro" : "PiSugar 2"); break;
	case POWER_SUPPLY_PROP_MANUFACTURER: val->strval = "PiSugar"; break;
	case POWER_SUPPLY_PROP_TEMP: val->intval = data->temperature; break;
	case POWER_SUPPLY_PROP_VOLTAGE_NOW: val->intval = data->voltage_uv; break;
	default: mutex_unlock(&data->lock); return -EINVAL;
	}
	mutex_unlock(&data->lock);
	return 0;
}

static enum power_supply_property ac_properties[] = {
	POWER_SUPPLY_PROP_ONLINE,
};

static int ac_get_property(struct power_supply *psy,
			   enum power_supply_property prop,
			   union power_supply_propval *val)
{
	struct pisugar_data *data = power_supply_get_drvdata(psy);

	if (prop != POWER_SUPPLY_PROP_ONLINE)
		return -EINVAL;
	mutex_lock(&data->lock);
	val->intval = data->online;
	mutex_unlock(&data->lock);
	return 0;
}

static const struct power_supply_desc battery_desc = {
	.name = "BAT0", .type = POWER_SUPPLY_TYPE_BATTERY,
	.properties = battery_properties,
	.num_properties = ARRAY_SIZE(battery_properties),
	.get_property = battery_get_property,
};

static const struct power_supply_desc ac_desc = {
	.name = "AC0", .type = POWER_SUPPLY_TYPE_MAINS,
	.properties = ac_properties,
	.num_properties = ARRAY_SIZE(ac_properties),
	.get_property = ac_get_property,
};

static int detect_hardware(struct pisugar_data *data)
{
	struct i2c_client *client;
	int v5209, v5312;
	int ver, mode;

	client = i2c_new_dummy_device(data->adapter, pisugar3_addr);
	if (IS_ERR(client))
		return PTR_ERR(client);
	ver = read_reg(client, 0x00);
	mode = read_reg(client, 0x01);
	if (ver == 3 && mode == 0x0f) {
		data->client = client;
		data->model = PISUGAR3;
		return 0;
	}
	i2c_unregister_device(client);

	client = i2c_new_dummy_device(data->adapter, PISUGAR2_ADDR);
	if (IS_ERR(client))
		return PTR_ERR(client);
	v5209 = ip5209_voltage(client);
	v5312 = ip5312_voltage(client);
	if (pisugar2_model == 0)
		data->model = PISUGAR2_IP5209;
	else if (pisugar2_model == 1)
		data->model = PISUGAR2_IP5312;
	else if (voltage_is_plausible(v5312))
		data->model = PISUGAR2_IP5312;
	else if (voltage_is_plausible(v5209))
		data->model = PISUGAR2_IP5209;
	else {
		i2c_unregister_device(client);
		return -ENODEV;
	}
	data->client = client;
	return 0;
}

static int __init pisugar_init(void)
{
	struct power_supply_config battery_cfg = { .drv_data = &pisugar };
	struct power_supply_config ac_cfg = { .drv_data = &pisugar };
	static char *supplied_to[] = { "BAT0" };
	int ret;

	mutex_init(&pisugar.lock);
	pisugar.adapter = i2c_get_adapter(i2c_bus);
	if (!pisugar.adapter)
		return -ENODEV;
	if (!i2c_check_functionality(pisugar.adapter, I2C_FUNC_SMBUS_BYTE_DATA)) {
		ret = -EOPNOTSUPP;
		goto put_adapter;
	}
	ret = detect_hardware(&pisugar);
	if (ret)
		goto put_adapter;

	mutex_lock(&pisugar.lock);
	ret = pisugar.model == PISUGAR3 ? update_pisugar3(&pisugar) :
		update_pisugar2(&pisugar);
	mutex_unlock(&pisugar.lock);
	if (ret)
		goto unregister_client;

	pisugar.battery = power_supply_register(NULL, &battery_desc, &battery_cfg);
	if (IS_ERR(pisugar.battery)) {
		ret = PTR_ERR(pisugar.battery);
		goto unregister_client;
	}
	ac_cfg.supplied_to = supplied_to;
	ac_cfg.num_supplicants = ARRAY_SIZE(supplied_to);
	pisugar.ac = power_supply_register(NULL, &ac_desc, &ac_cfg);
	if (IS_ERR(pisugar.ac)) {
		ret = PTR_ERR(pisugar.ac);
		goto unregister_battery;
	}

	INIT_DELAYED_WORK(&pisugar.monitor_work, pisugar_monitor);
	schedule_delayed_work(&pisugar.monitor_work, POLL_INTERVAL);
	pr_info("pisugar_battery: detected %s on i2c-%d\n",
		pisugar.model == PISUGAR3 ? "PiSugar 3" :
		(pisugar.model == PISUGAR2_IP5312 ? "PiSugar 2 Pro" : "PiSugar 2"),
		i2c_bus);
	return 0;

unregister_battery:
	power_supply_unregister(pisugar.battery);
unregister_client:
	i2c_unregister_device(pisugar.client);
put_adapter:
	i2c_put_adapter(pisugar.adapter);
	return ret;
}

static void __exit pisugar_exit(void)
{
	cancel_delayed_work_sync(&pisugar.monitor_work);
	power_supply_unregister(pisugar.ac);
	power_supply_unregister(pisugar.battery);
	i2c_unregister_device(pisugar.client);
	i2c_put_adapter(pisugar.adapter);
}

module_init(pisugar_init);
module_exit(pisugar_exit);

MODULE_AUTHOR("The PiSugar Team <pisugar.zero@gmail.com>");
MODULE_DESCRIPTION("Auto-detecting PiSugar 2/3 power supply driver");
MODULE_LICENSE("GPL");

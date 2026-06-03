#include <linux/fs.h>
#include <linux/ktime.h>
#include <linux/miscdevice.h>
#include <linux/module.h>
#include <linux/mutex.h>
#include <linux/uaccess.h>

#include "adc_sensor_probe.h"

#define ADC_SENSOR_PROBE_NAME "adc_sensor_probe"
#define ADC_SENSOR_PROBE_EVENT_BYTES 160

static DEFINE_MUTEX(adc_sensor_lock);
static char adc_sensor_event[ADC_SENSOR_PROBE_EVENT_BYTES];
static bool adc_sensor_armed;
static u64 adc_sensor_seq;
static u64 adc_sensor_drops;

static void adc_sensor_emit_selftest_locked(void)
{
	u64 mono_ns = ktime_get_ns();

	adc_sensor_seq++;
	scnprintf(adc_sensor_event, sizeof(adc_sensor_event),
		  "{\"type\":\"self_test\",\"monotonic_ns\":%llu,\"seq\":%llu,\"armed\":%s}\n",
		  mono_ns, adc_sensor_seq, adc_sensor_armed ? "true" : "false");
}

static ssize_t adc_sensor_read(struct file *file, char __user *buf, size_t count,
			       loff_t *ppos)
{
	ssize_t ret;

	mutex_lock(&adc_sensor_lock);
	if (adc_sensor_event[0] == '\0')
		adc_sensor_emit_selftest_locked();
	ret = simple_read_from_buffer(buf, count, ppos, adc_sensor_event,
				      strnlen(adc_sensor_event,
					      sizeof(adc_sensor_event)));
	mutex_unlock(&adc_sensor_lock);
	return ret;
}

static long adc_sensor_ioctl(struct file *file, unsigned int cmd,
			     unsigned long arg)
{
	long ret = 0;

	mutex_lock(&adc_sensor_lock);
	switch (cmd) {
	case ADC_SENSOR_PROBE_IOC_ARM:
		adc_sensor_armed = true;
		adc_sensor_emit_selftest_locked();
		break;
	case ADC_SENSOR_PROBE_IOC_DISARM:
		adc_sensor_armed = false;
		adc_sensor_emit_selftest_locked();
		break;
	case ADC_SENSOR_PROBE_IOC_SELFTEST:
		adc_sensor_emit_selftest_locked();
		break;
	case ADC_SENSOR_PROBE_IOC_GET_DROPS:
		if (copy_to_user((void __user *)arg, &adc_sensor_drops,
				 sizeof(adc_sensor_drops)))
			ret = -EFAULT;
		break;
	default:
		ret = -ENOTTY;
		break;
	}
	mutex_unlock(&adc_sensor_lock);
	return ret;
}

static const struct file_operations adc_sensor_fops = {
	.owner = THIS_MODULE,
	.read = adc_sensor_read,
	.unlocked_ioctl = adc_sensor_ioctl,
#ifdef CONFIG_COMPAT
	.compat_ioctl = adc_sensor_ioctl,
#endif
	.llseek = noop_llseek,
};

static struct miscdevice adc_sensor_miscdev = {
	.minor = MISC_DYNAMIC_MINOR,
	.name = ADC_SENSOR_PROBE_NAME,
	.fops = &adc_sensor_fops,
	.mode = 0600,
};

static int __init adc_sensor_probe_init(void)
{
	int ret;

	mutex_lock(&adc_sensor_lock);
	adc_sensor_armed = false;
	adc_sensor_seq = 0;
	adc_sensor_drops = 0;
	adc_sensor_emit_selftest_locked();
	mutex_unlock(&adc_sensor_lock);

	ret = misc_register(&adc_sensor_miscdev);
	if (ret)
		pr_err("%s: misc_register failed: %d\n",
		       ADC_SENSOR_PROBE_NAME, ret);
	else
		pr_info("%s: loaded\n", ADC_SENSOR_PROBE_NAME);
	return ret;
}

static void __exit adc_sensor_probe_exit(void)
{
	misc_deregister(&adc_sensor_miscdev);
	pr_info("%s: unloaded\n", ADC_SENSOR_PROBE_NAME);
}

module_init(adc_sensor_probe_init);
module_exit(adc_sensor_probe_exit);

MODULE_LICENSE("Dual MIT/GPL");
MODULE_AUTHOR("adc-targetd");
MODULE_DESCRIPTION("Optional adc-targetd safe self-test event producer");

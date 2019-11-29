#![deny(unsafe_code)]
#![no_main]
#![no_std]

pub use cortex_m::{iprint, iprintln, peripheral};
use cortex_m_semihosting::hprintln;
use panic_semihosting as _;
use rtfm::cyccnt::U32Ext;
use stm32f3xx_hal::gpio::{gpioe, GpioExt, Output, PushPull};
use stm32f3xx_hal::hal::digital::v2::*;
use stm32f3xx_hal::rcc::*;

const ON_TIME: u32 = 1_000_000;
const OFF_TIME: u32 = 2_000_000;
#[rtfm::app(device = stm32f3xx_hal::stm32, monotonic = rtfm::cyccnt::CYCCNT, peripherals = true)]
const APP: () = {
    struct Resources {
        #[init(0)]
        current_led: usize,
        #[init(false)]
        led_state: bool,
        leds: [gpioe::PEx<Output<PushPull>>; 1],
    }

    #[init(schedule=[blink])]
    fn init(ctx: init::Context) -> init::LateResources {
        let mut rcc = ctx.device.RCC.constrain();
        let mut gpioe = ctx.device.GPIOE.split(&mut rcc.ahb);

        let pe9 = gpioe
            .pe9
            .into_push_pull_output(&mut gpioe.moder, &mut gpioe.otyper)
            .downgrade();
        let leds = [pe9];
        ctx.schedule.blink(ctx.start + OFF_TIME.cycles()).unwrap();
        init::LateResources { leds }
    }

    #[task(resources=[led_state, leds, current_led], schedule=[blink])]
    fn blink(c: blink::Context) {
        let led_state = !*c.resources.led_state;
        *c.resources.led_state = led_state;
        let current_led = *c.resources.current_led;
        let leds = c.resources.leds;
        match led_state {
            false => {
                leds[current_led].set_low().unwrap();
                c.schedule.blink(c.scheduled + OFF_TIME.cycles()).unwrap();
            }
            true => {
                leds[current_led].set_high().unwrap();
                c.schedule.blink(c.scheduled + ON_TIME.cycles()).unwrap();
            }
        }
    }
    extern "C" {
        fn SPI1();
    }
};

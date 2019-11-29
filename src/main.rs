#![no_main]
#![no_std]

use panic_halt as _;
use rtfm::cyccnt::U32Ext;
use stm32f3xx_hal::gpio::*;
use stm32f3xx_hal::hal::digital::v2::*;
use stm32f3xx_hal::rcc::*;
use stm32f3xx_hal::stm32;

const ON_TIME: u32 = 1_000_000;
const OFF_TIME: u32 = 2_000_000;
const DEBOUNCE_DELAY: u32 = 160_000;

fn init_leds(gpioe: stm32::GPIOE, ahb: &mut AHB) -> [gpioe::PEx<Output<PushPull>>; 8] {
    let mut gpioe = gpioe.split(ahb);

    let leds = [
        gpioe
            .pe9
            .into_push_pull_output(&mut gpioe.moder, &mut gpioe.otyper)
            .downgrade(),
        gpioe
            .pe10
            .into_push_pull_output(&mut gpioe.moder, &mut gpioe.otyper)
            .downgrade(),
        gpioe
            .pe11
            .into_push_pull_output(&mut gpioe.moder, &mut gpioe.otyper)
            .downgrade(),
        gpioe
            .pe12
            .into_push_pull_output(&mut gpioe.moder, &mut gpioe.otyper)
            .downgrade(),
        gpioe
            .pe13
            .into_push_pull_output(&mut gpioe.moder, &mut gpioe.otyper)
            .downgrade(),
        gpioe
            .pe14
            .into_push_pull_output(&mut gpioe.moder, &mut gpioe.otyper)
            .downgrade(),
        gpioe
            .pe15
            .into_push_pull_output(&mut gpioe.moder, &mut gpioe.otyper)
            .downgrade(),
        gpioe
            .pe8
            .into_push_pull_output(&mut gpioe.moder, &mut gpioe.otyper)
            .downgrade(),
    ];
    leds
}

#[rtfm::app(device = stm32f3xx_hal::stm32, monotonic = rtfm::cyccnt::CYCCNT, peripherals = true)]
const APP: () = {
    struct Resources {
        #[init(0)]
        current_led: usize,
        #[init(false)]
        led_state: bool,
        #[init(false)]
        btn_pressed: bool,
        leds: [gpioe::PEx<Output<PushPull>>; 8],
        button: gpioa::PA0<Input<PullDown>>,
        exti: stm32::EXTI,
    }

    #[init(schedule=[blink])]
    fn init(ctx: init::Context) -> init::LateResources {
        let mut rcc = ctx.device.RCC.constrain();

        let mut gpioa = ctx.device.GPIOA.split(&mut rcc.ahb);
        let button = gpioa
            .pa0
            .into_pull_down_input(&mut gpioa.moder, &mut gpioa.pupdr);

        let leds = init_leds(ctx.device.GPIOE, &mut rcc.ahb);
        let syscfg = ctx.device.SYSCFG;
        syscfg
            .exticr1
            .modify(|_, w| unsafe { w.exti0().bits(0b000) });
        let exti = ctx.device.EXTI;
        exti.imr1.modify(|_, w| w.mr0().set_bit());
        exti.rtsr1.modify(|_, w| w.tr0().set_bit());
        ctx.schedule.blink(ctx.start + OFF_TIME.cycles()).unwrap();
        unsafe { stm32::NVIC::unmask(stm32::Interrupt::EXTI0) };
        init::LateResources { leds, exti, button }
    }

    #[task(resources=[led_state, leds, current_led, btn_pressed], schedule=[blink])]
    fn blink(c: blink::Context) {
        let led_state = !*c.resources.led_state;
        *c.resources.led_state = led_state;
        let current_led = *c.resources.current_led;
        let leds = c.resources.leds;
        match led_state {
            false => {
                leds[current_led].set_low().unwrap();
                if *c.resources.btn_pressed == true {
                    *c.resources.btn_pressed = false;
                    *c.resources.current_led = (current_led + 1) % leds.len();
                }
                c.schedule.blink(c.scheduled + OFF_TIME.cycles()).unwrap();
            }
            true => {
                leds[current_led].set_high().unwrap();
                c.schedule.blink(c.scheduled + ON_TIME.cycles()).unwrap();
            }
        }
    }

    #[task(resources = [btn_pressed])]
    fn btn_event(c: btn_event::Context) {
        *c.resources.btn_pressed = true;
    }

    #[task(binds = EXTI0, resources = [exti], schedule = [btn_event])]
    fn exti0(c: exti0::Context) {
        c.resources.exti.pr1.modify(|_, w| w.pr0().set_bit());
        c.schedule
            .btn_event(c.start + DEBOUNCE_DELAY.cycles())
            .unwrap_or(());
    }
    extern "C" {
        fn SPI1();
    }
};

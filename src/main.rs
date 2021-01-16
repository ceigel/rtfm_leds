#![no_main]
#![no_std]

use core::time::Duration;
pub use cortex_m::iprintln;
use cortex_m::peripheral::ITM;
use heapless::mpmc::Q8;
use panic_halt as _;
use rtfm::cyccnt::{Instant, U32Ext};
use stm32f3xx_hal::gpio::*;
use stm32f3xx_hal::hal::digital::v2::*;
use stm32f3xx_hal::stm32;
use stm32f3xx_hal::time::*;
use stm32f3xx_hal::{flash::*, rcc::*};

const ON_TIME: Duration = Duration::from_millis(250);
const OFF_TIME: Duration = Duration::from_millis(250);
const HOLD_TIME: Duration = Duration::from_millis(1000);
const DOUBLE_CLICK_TIME: Duration = Duration::from_millis(700);
const FLASH_TIME: Duration = Duration::from_millis(300);

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

#[derive(Debug)]
pub enum ButtonEvents {
    Click,
    Hold,
    DoubleClick,
}

pub struct CyclesComputer {
    frequency: Hertz,
}

impl CyclesComputer {
    pub fn new(frequency: Hertz) -> Self {
        CyclesComputer { frequency }
    }

    pub fn to_cycles(&self, duration: Duration) -> rtfm::cyccnt::Duration {
        let s_part = (duration.as_secs() as u32) * self.frequency.0;
        let mms_part = (duration.subsec_micros() / 1000) * (self.frequency.0 / 1000);
        (s_part + mms_part).cycles()
    }
}

fn compute_next(current_led: usize, increment: bool, max: usize) -> usize {
    if increment {
        (current_led + 1) % max
    } else {
        if current_led != 0 {
            current_led - 1
        } else {
            max - 1
        }
    }
}

#[rtfm::app(device = stm32f3xx_hal::stm32, monotonic = rtfm::cyccnt::CYCCNT, peripherals = true)]
const APP: () = {
    struct Resources {
        #[init(0)]
        current_led: usize,
        #[init(0)]
        next_led: usize,
        #[init(false)]
        led_state: bool,
        #[init(true)]
        next_led_increment: bool,
        last_raising: Instant,
        last_falling: Instant,
        time_computer: CyclesComputer,
        leds: [gpioe::PEx<Output<PushPull>>; 8],
        button: gpioa::PA0<Input<PullDown>>,
        exti: stm32::EXTI,
        itm: ITM,
        button_events: Q8<ButtonEvents>,
    }

    #[init(schedule=[blink])]
    fn init(mut ctx: init::Context) -> init::LateResources {
        // Initialize (enable) the monotonic timer (CYCCNT)
        ctx.core.DWT.enable_cycle_counter();
        let mut flash = ctx.device.FLASH.constrain();
        let mut rcc = ctx.device.RCC.constrain();
        let clocks = rcc
            .cfgr
            .use_hse(MegaHertz(8))
            .sysclk(MegaHertz(64))
            .pclk1(MegaHertz(32))
            .pclk2(MegaHertz(64))
            .hclk(MegaHertz(64))
            .freeze(&mut flash.acr);
        let time_computer = CyclesComputer::new(clocks.sysclk());

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
        exti.ftsr1.modify(|_, w| w.tr0().set_bit());
        let mut itm = ctx.core.ITM;
        let last_raising = ctx.start;
        let last_falling = ctx.start;
        let button_events = Q8::new();
        iprintln!(&mut itm.stim[0], "Hello");
        ctx.schedule
            .blink(ctx.start + time_computer.to_cycles(OFF_TIME))
            .unwrap();
        init::LateResources {
            last_raising,
            last_falling,
            time_computer,
            leds,
            exti,
            button,
            button_events,
            itm,
        }
    }

    #[idle]
    fn idle(_ctx: idle::Context) -> ! {
        loop {}
    }

    #[task(resources=[itm, current_led, leds, next_led, button_events, next_led_increment], spawn=[flash, blink])]
    fn dispatcher(c: dispatcher::Context) {
        let itm = c.resources.itm;
        if let Some(button_event) = c.resources.button_events.dequeue() {
            iprintln!(&mut itm.stim[0], "Got event {:?}", button_event);
            match button_event {
                ButtonEvents::Hold => {
                    c.spawn.flash().unwrap();
                    return;
                }
                ButtonEvents::Click => {
                    *c.resources.next_led = compute_next(
                        *c.resources.current_led,
                        *c.resources.next_led_increment,
                        c.resources.leds.len(),
                    );
                }
                ButtonEvents::DoubleClick => {
                    *c.resources.next_led_increment = !*c.resources.next_led_increment;
                    *c.resources.next_led = compute_next(
                        *c.resources.current_led,
                        *c.resources.next_led_increment,
                        c.resources.leds.len(),
                    );
                }
            }
        }
        c.spawn.blink().unwrap();
    }

    #[task(resources=[itm, leds, time_computer], schedule=[flash], spawn=[dispatcher])]
    fn flash(c: flash::Context) {
        static mut LEDS_STATE: bool = true;
        static mut CYCLE_COUNT: usize = 0;
        for led in c.resources.leds {
            match LEDS_STATE {
                false => led.set_low().ok(),
                true => led.set_high().ok(),
            };
        }
        *LEDS_STATE = !*LEDS_STATE;
        let stim = &mut c.resources.itm.stim[0];
        iprintln!(stim, "Flash: Cycle count: {}", CYCLE_COUNT);
        *CYCLE_COUNT += 1;
        if *CYCLE_COUNT == 6 {
            *CYCLE_COUNT = 0;
            c.spawn.dispatcher().unwrap();
        } else {
            c.schedule
                .flash(c.scheduled + c.resources.time_computer.to_cycles(FLASH_TIME))
                .unwrap();
        }
    }

    #[task(resources=[itm, led_state, leds, current_led, next_led, button_events, time_computer], schedule=[dispatcher])]
    fn blink(c: blink::Context) {
        let led_state = !*c.resources.led_state;
        *c.resources.led_state = led_state;
        let current_led = *c.resources.current_led;
        let leds = c.resources.leds;
        match led_state {
            false => {
                leds[current_led].set_low().unwrap();
                *c.resources.current_led = *c.resources.next_led;
                c.schedule
                    .dispatcher(c.scheduled + c.resources.time_computer.to_cycles(OFF_TIME))
                    .unwrap();
            }
            true => {
                leds[current_led].set_high().unwrap();
                c.schedule
                    .dispatcher(c.scheduled + c.resources.time_computer.to_cycles(ON_TIME))
                    .unwrap();
            }
        }
    }

    #[task(resources = [itm, button_events, last_raising, last_falling, time_computer])]
    fn btn_hold(c: btn_hold::Context) {
        let itm = c.resources.itm;
        let v2 = c.resources.time_computer.to_cycles(HOLD_TIME);
        if c.scheduled.duration_since(*c.resources.last_raising) >= v2
            && *c.resources.last_raising > *c.resources.last_falling
        {
            iprintln!(&mut itm.stim[0], "add event Hold:");
            c.resources.button_events.enqueue(ButtonEvents::Hold).ok();
        }
    }

    #[task(binds = EXTI0, resources = [itm, button_events, time_computer, button, exti, last_raising, last_falling], schedule=[btn_hold])]
    fn exti0(c: exti0::Context) {
        let itm = c.resources.itm;
        c.resources.exti.pr1.modify(|_, w| w.pr0().set_bit());
        if (*c.resources.button).is_high().unwrap_or(false) {
            *c.resources.last_raising = c.start;
            if c.start.duration_since(*c.resources.last_falling)
                < c.resources.time_computer.to_cycles(DOUBLE_CLICK_TIME)
            {
                iprintln!(&mut itm.stim[0], "add event DoubleClick:");
                c.resources
                    .button_events
                    .enqueue(ButtonEvents::DoubleClick)
                    .ok();
            } else {
                iprintln!(&mut itm.stim[0], "add event Click:");
                c.resources.button_events.enqueue(ButtonEvents::Click).ok();
            }
            c.schedule
                .btn_hold(c.start + c.resources.time_computer.to_cycles(HOLD_TIME))
                .ok();
        } else {
            *c.resources.last_falling = c.start;
        }
    }
    extern "C" {
        fn SPI1();
    }
};

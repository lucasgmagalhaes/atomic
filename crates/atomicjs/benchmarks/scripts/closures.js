function makeCounter() {
    let count = 0;
    return function () {
        return ++count;
    };
}
const counter = makeCounter();
let total = 0;
for (let i = 0; i < 1000000; i++) {
    total += counter();
}
total;
